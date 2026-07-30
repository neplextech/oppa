---
name: openprinter-server
description: >
  @openprinter/server fundamentals for agents building or integrating
  an OpenPrinter-compatible server endpoint. Load this skill when you
  encounter server SDK imports, need to host agent sessions over
  WebSocket or another transport, deliver print jobs, handle printer
  inventory callbacks, or implement session lifecycle and heartbeat
  management.
---

# @openprinter/server — Agent Skill

> **Package**: `@openprinter/server` · **npm**:
> `https://www.npmjs.com/package/@openprinter/server` **Version**:
> 0.2.x · **TypeScript strict mode**

## What this package is

`@openprinter/server` is a **framework-neutral protocol-session SDK**
for hosting OpenPrinter-compatible agents.

It owns:

- Protocol handshake and version negotiation
- Heartbeat scheduling and timeout enforcement
- Message validation (delegates to `@openprinter/protocol`)
- Per-session state (lifecycle, current printer inventory)
- Ordered message processing and typed callbacks

It does **not** own:

- HTTP servers, WebSocket listeners, or any transport layer
- Authentication, access-token parsing, or credential storage
- A global connection registry or routing table
- Cluster coordination or broker infrastructure
- Durable job queues, retry scheduling, or database access
- Tenant models, user sessions, or printer-routing policy

The host application owns all of that. The SDK only needs a `send`
callback and a `close` callback to operate over any transport.

---

## Installation

```bash
npm install @openprinter/server @openprinter/protocol
# or
pnpm add @openprinter/server @openprinter/protocol
```

```ts
import { createOpenPrinterServer } from '@openprinter/server';
```

---

## Quick start (WebSocket example)

```ts
import { WebSocketServer } from 'ws';
import { createOpenPrinterServer } from '@openprinter/server';
import type { PrintJob } from '@openprinter/protocol';

const server = createOpenPrinterServer<{ tenantId: string }>({
  brand: { name: 'My Print Service' },
  serverId: 'my-service-v1',
  serverVersion: '1.0.0',

  onAgentConnected({ session, agent }) {
    console.log(
      'Agent connected:',
      agent.agentId,
      'session:',
      session.sessionId,
    );
  },

  onAgentDisconnected({ agent, reason }) {
    console.log('Agent disconnected:', agent.agentId, reason);
  },

  onPrintersChanged({ agent, kind, printers }) {
    console.log(
      `${agent.agentId} printer ${kind}:`,
      printers.map((p) => p.id),
    );
  },

  onJobReceived({ agent, message }) {
    const { jobId, status } = message.payload;
    console.log(`Job ${jobId} durably received by ${agent.agentId}`);
  },

  onJobSubmitted({ agent, message }) {
    const { jobId, printerId } = message.payload;
    console.log(`Job ${jobId} submitted to printer ${printerId}`);
  },

  onJobFailed({ agent, message }) {
    const { jobId, error } = message.payload;
    console.error(
      `Job ${jobId} failed (retryable=${error.retryable}):`,
      error.message,
    );
  },

  onProtocolError({ agentId, code, error }) {
    console.error(
      `Protocol error from ${agentId} [${code}]:`,
      error.message,
    );
  },

  onCallbackError({ callback, error }) {
    console.error(`Callback ${callback} threw:`, error);
  },
});

const wss = new WebSocketServer({ port: 8080 });

wss.on('connection', (ws, req) => {
  // Host application authenticates the request here (JWT, API key, etc.)
  const agentId = authenticate(req); // your auth logic
  if (!agentId) {
    ws.close(4001, 'Unauthorized');
    return;
  }

  const session = server.accept({
    identity: { agentId, metadata: { tenantId: 'tenant-abc' } },
    transport: {
      send: (msg) => ws.send(msg),
      close: ({ reason }) => ws.close(4000, reason),
    },
  });

  ws.on('message', (data) => {
    void session.receive(data as string | Uint8Array);
  });

  ws.on('close', () => {
    void session.transportClosed({ reason: 'peer-closed' });
  });

  ws.on('error', (err) => {
    void session.transportClosed({
      reason: 'transport-error',
      detail: err.message,
    });
  });
});
```

---

## Core API

### `createOpenPrinterServer(options)`

Creates a session factory. Call once per server process.

```ts
import { createOpenPrinterServer } from '@openprinter/server';

const server = createOpenPrinterServer<YourMetadata>(options);
```

Returns an `OpenPrinterServer<Metadata>` with one method: `accept()`.

---

### `server.accept(input)`

Opens one independent protocol session over a host-owned transport.

```ts
const session = server.accept({
  identity: {
    agentId: string,         // stable identity from your auth layer
    metadata?: YourMetadata, // any host-owned context; returned in every callback
  },
  sessionId?: string,        // optional host-assigned session ID (UUID generated if omitted)
  transport: {
    send(message: string): Awaitable<void>,    // hand encoded frame to your transport
    close(req: { reason: string; detail?: string }): Awaitable<void>, // close transport
  },
});
```

Returns an `OpenPrinterSession<Metadata>` immediately. The handshake
begins asynchronously when the first `agent.hello` arrives via
`session.receive()`.

---

## `OpenPrinterSession<Metadata>` methods

### Receiving inbound frames

```ts
// Call this for every raw message from the agent transport.
// Concurrent calls are serialized in invocation order.
await session.receive(message: string | Uint8Array);

// Call when the transport closes externally (WebSocket close, broker disconnect, etc.)
await session.transportClosed({ reason: 'peer-closed' | 'transport-error', detail?: string });
```

### Sending server messages

```ts
// Send a validated print job to this agent.
const result = await session.sendJob(job: PrintJob): Promise<DeliveryResult>;

// Ask the agent to report a full printer inventory.
const result = await session.requestPrinters(): Promise<DeliveryResult>;

// Ask the agent to cancel a job not yet submitted.
const result = await session.cancelJob({ jobId: string, reason?: string }): Promise<DeliveryResult>;

// Notify the agent that host configuration changed.
const result = await session.invalidateConfiguration({
  scope: 'agent' | 'printers' | 'all',
  revision?: string,
}): Promise<DeliveryResult>;

// Send any application-level server message (not hello/heartbeat/disconnect).
const result = await session.send(message: OpenPrinterApplicationMessage): Promise<DeliveryResult>;

// Gracefully disconnect this agent.
const sent: boolean = await session.disconnect({
  code?: string,       // stable identifier delivered to the agent
  reason?: string,
  reconnect?: boolean,
  retryAfterMs?: number,
});
```

### Inspecting session state

```ts
session.identity; // { agentId, metadata? }
session.sessionId; // string
session.state; // 'handshaking' | 'connected' | 'closing' | 'closed'

session.getAgent(); // ConnectedAgent<Metadata> | null (null before handshake or after disconnect)
session.getPrinters(); // readonly PrinterDescriptor[]
```

---

## `DeliveryResult`

All `send*` methods return `Promise<DeliveryResult>`. Check
`result.ok` before assuming delivery.

```ts
type DeliveryResult = DeliverySuccess | DeliveryFailure;

interface DeliverySuccess {
  ok: true;
  agentId: string;
  sessionId: string;
  messageId: string; // UUID used in the wire envelope
  sentAt: string; // ISO 8601 UTC
}

interface DeliveryFailure {
  ok: false;
  agentId: string;
  reason:
    | 'agent-offline'
    | 'session-not-ready'
    | 'connection-closed'
    | 'transport-error';
  retryable: true; // always true; host decides retry policy
}
```

The SDK never queues failed messages. If `result.ok` is `false`, the
host application must decide whether to retry (e.g. via an
application-owned queue).

---

## `OpenPrinterServerOptions<Metadata>`

### Required

```ts
brand: {
  name: string;
} // display identity sent to agents in server.hello; no external URLs
```

### Optional identity and limits

```ts
serverId?: string          // default: generated UUID
serverVersion?: string     // default: '0.0.0'
maxMessageBytes?: number   // default: 2 097 152 (2 MB)
handshakeTimeoutMs?: number  // time allowed for first agent.hello; default: 30 000
transportTimeoutMs?: number  // max time for transport send/close; default: 10 000
callbackTimeoutMs?: number   // max time for any user callback; default: 30 000
heartbeatIntervalMs?: number // how often to send server.heartbeat; default: 30 000
heartbeatTimeoutMs?: number  // how long to wait for agent.heartbeat response; default: 15 000
```

### Callbacks

All callbacks are optional and isolated. If one throws,
`onCallbackError` is called instead of crashing the session.

```ts
onAgentConnected?(event: AgentConnectedEvent<Metadata>): Awaitable<void>
// Called when the handshake completes. Use this to start job delivery.
// event.session — the live session
// event.agent   — ConnectedAgent snapshot

onAgentDisconnected?(event: AgentDisconnectedEvent<Metadata>): Awaitable<void>
// Called exactly once when a connected session ends.
// event.reason — 'peer-closed' | 'heartbeat-timeout' | 'protocol-error' | 'server-disconnect' | 'transport-error'

onAuthenticationMetadata?(event: AgentMessageEvent<Metadata, AgentMessageOf<'agent.authentication_metadata'>>): Awaitable<void>
// Called for optional non-secret agent auth context.
// event.message.payload — { method, subject?, metadata? }

onPrintersChanged?(event: PrintersChangedEvent<Metadata>): Awaitable<void>
// Called after every inventory update.
// event.kind     — 'snapshot' | 'change'
// event.revision — monotonic integer from the agent
// event.printers — complete current inventory (post-update)
// event.message  — the raw protocol message that caused the update

onJobReceived?(event: AgentMessageEvent<Metadata, AgentMessageOf<'agent.job_received'>>): Awaitable<void>
// Agent has durably persisted the job. Safe to mark as acknowledged in your system.

onJobSubmitted?(event: AgentMessageEvent<Metadata, AgentMessageOf<'agent.job_submitted'>>): Awaitable<void>
// OS or printer backend accepted the job. NOT a guarantee of physical printing.
// Check event.message.payload.printerId to know which printer.

onJobFailed?(event: AgentMessageEvent<Metadata, AgentMessageOf<'agent.job_failed'>>): Awaitable<void>
// Job could not be submitted. Check error.retryable.

onDiagnostics?(event: AgentMessageEvent<Metadata, AgentMessageOf<'agent.diagnostics'>>): Awaitable<void>
// Bounded, sanitized operational summary from the agent.

onHeartbeatTimeout?(event: HeartbeatTimeoutEvent<Metadata>): Awaitable<void>
// Agent failed to respond to heartbeat in time. Session has already been made unavailable.

onProtocolError?(event: ServerProtocolErrorEvent<Metadata>): Awaitable<void>
// Inbound protocol invariant was rejected.
// event.code — 'handshake-timeout' | 'identity-mismatch' | 'invalid-message' |
//              'message-too-large' | 'unexpected-message' | 'unsupported-protocol-version'

onCallbackError?(event: CallbackErrorEvent): Awaitable<void>
// Another user callback threw. event.callback names which one, event.error is the thrown value.
```

---

## `ConnectedAgent<Metadata>` snapshot

Passed to most callbacks. It is an immutable snapshot at
callback-dispatch time.

```ts
interface ConnectedAgent<Metadata> {
  agentId: string;
  sessionId: string;
  metadata?: Readonly<Metadata>; // host-supplied at accept() time
  hello: Readonly<AgentHello>; // validated agent.hello payload
  connectedAt: string; // ISO 8601 UTC
  lastSeenAt: string; // ISO 8601 UTC (updated on every inbound message)
  printerRevision: number | null; // latest known inventory revision, null if none received
}
```

---

## Error classes

```ts
import {
  OpenPrinterServerError,
  OpenPrinterSessionError,
} from '@openprinter/server';

// Thrown by createOpenPrinterServer() for invalid options
class OpenPrinterServerError extends Error {
  readonly code: string;
}

// Thrown by session methods for invalid calls (e.g. missing required fields)
class OpenPrinterSessionError extends Error {
  readonly code: string;
}
```

---

## Multi-agent routing pattern

The SDK creates **independent sessions per connection**. Two sessions
with the same `agentId` are entirely independent. The host decides
which session is "active" and routes jobs accordingly.

```ts
const sessions = new Map<string, OpenPrinterSession<Metadata>>();

// On new connection:
const session = server.accept({ identity: { agentId }, transport });
sessions.set(agentId, session); // replaces previous session if any

// In onAgentDisconnected:
if (sessions.get(event.agent.agentId) === event.session) {
  sessions.delete(event.agent.agentId);
}

// Send a job to a specific agent:
const session = sessions.get(targetAgentId);
if (!session) {
  /* agent offline — enqueue in your own queue */ return;
}

const result = await session.sendJob(job);
if (!result.ok) {
  /* transport failed — enqueue retry */
}
```

---

## Sending a print job

A `PrintJob` must come from `@openprinter/protocol`:

```ts
import type { PrintJob, PrintDocument } from '@openprinter/protocol';

const document: PrintDocument = {
  width: 80,
  sections: [
    {
      type: 'text',
      value: 'Hello, World!',
      align: 'center',
      bold: true,
    },
    { type: 'divider' },
    { type: 'row', left: 'Total', right: '$12.50' },
    { type: 'feed', lines: 3 },
    { type: 'cut' },
  ],
};

const job: PrintJob = {
  jobId: crypto.randomUUID(),
  idempotencyKey: crypto.randomUUID(), // server assigns this
  printerId: 'printer-uuid-from-inventory',
  createdAt: new Date().toISOString(),
  document,
  metadata: { orderId: '12345' }, // opaque, max 32 entries
};

const result = await session.sendJob(job);
if (!result.ok) {
  // Put in your retry queue with result.reason
}
```

**Critical**: `printerId` must match an `id` from the agent's current
inventory. Validate before sending.

---

## Job lifecycle states

```
server sends  → server.print_job
agent replies → agent.job_received   (durable; safe to ACK in your system)
agent replies → agent.job_submitted  (OS/printer accepted; not physical printing)
agent replies → agent.job_failed     (check error.retryable)
```

Use `idempotencyKey` to correlate all three responses back to your
original job. The key echoed back in `job_received`, `job_submitted`,
and `job_failed` is always the one the server assigned.

---

## Important invariants

- `'submitted'` status does NOT mean paper was physically produced.
  Never expose it as "printed" to end users without additional
  confirmation.
- `received` → `submitted` or `received` → `failed` are the expected
  transitions. You may also get `failed` without a preceding
  `received` if the agent crashes before writing to stable storage.
- The SDK never queues failed deliveries. All retry logic belongs in
  the host application.
- Authentication tokens must live in the transport handshake (e.g.
  `Authorization` HTTP header). They must never appear in
  `agent.authentication_metadata`.
- Sessions with the same `agentId` remain independent. The host is
  responsible for deduplication and replacement.

---

## Workspace commands

```bash
pnpm --filter @openprinter/server check      # typecheck + test
pnpm --filter @openprinter/server test       # vitest
pnpm --filter @openprinter/server build      # compile TypeScript → dist/
pnpm --filter @openprinter/protocol build    # must build protocol first if types are stale
```
