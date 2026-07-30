---
name: openprinter-protocol
description: >
  @openprinter/protocol fundamentals for agents integrating or
  implementing the OpenPrinter wire protocol. Load this skill when you
  encounter protocol imports, need to parse or encode agent/server
  messages, work with PrintDocument or PrintJob structures, or must
  understand protocol version constraints and limits.
---

# @openprinter/protocol — Agent Skill

> **Package**: `@openprinter/protocol` · **npm**:
> `https://www.npmjs.com/package/@openprinter/protocol` **Version**:
> protocol v1 · **TypeScript strict mode**

## What this package is

`@openprinter/protocol` is the **canonical source of truth** for the
OpenPrinter wire protocol. It provides:

- TypeBox schemas for every message type in both directions
- Runtime-validated codec functions (parse, decode, encode)
- TypeScript types inferred directly from the schemas (no drift
  possible)
- Shared constants and structured error classes
- A generated JSON Schema at `protocol/schema/openprinter.schema.json`

It has **no runtime dependencies** beyond `@sinclair/typebox/value`
for validation. Use it in agents, servers, test harnesses, CLI tools,
and documentation generators.

---

## Installation

```bash
npm install @openprinter/protocol
# or
pnpm add @openprinter/protocol
```

```ts
import {
  decodeAgentMessage,
  encodeServerMessage,
  PROTOCOL_VERSION,
} from '@openprinter/protocol';
```

---

## Protocol overview

OpenPrinter is a **bidirectional, version-negotiated, message-passing
protocol** over any reliable ordered transport (WebSocket, TCP, broker
queue, etc.).

- **Agents** (e.g. OPPA desktop app) send `agent.*` messages.
- **Servers** send `server.*` messages.
- Every message carries `protocolVersion`, `messageId`, `sentAt`,
  `type`, and `payload`.
- Correlated messages also carry `correlationId` matching a prior
  `messageId`.
- Maximum wire size: **2 MB** (UTF-8 JSON).

The handshake sequence:

1. Agent connects and sends `agent.hello`.
2. Server replies with `server.hello` (correlated to `agent.hello`).
3. Agent optionally sends `agent.authentication_metadata`.
4. Both sides exchange operational messages.
5. Server sends `server.heartbeat` periodically; agent responds with
   `agent.heartbeat`.
6. Either side terminates; server may send `server.disconnect` first.

---

## Message types

### Agent → Server messages

All share the envelope:
`{ protocolVersion, messageId, sentAt, type, payload }`. Correlated
messages also include `correlationId`.

#### `agent.hello`

Initial handshake and version advertisement. **Must be the first
message.**

```ts
type AgentHello = {
  agentId: string; // stable agent identity (UUID recommended)
  agentVersion: string; // semver or build string
  productId: string; // branded product identifier
  productVersion: string; // product version string
  supportedProtocolVersions: number[]; // [1] for current agents
};
```

#### `agent.authentication_metadata`

Optional non-secret authentication context. Tokens must NOT appear
here.

```ts
type AuthenticationMetadata = {
  method: 'oauth2' | 'api_key' | 'none';
  subject?: string; // e.g. user or tenant identifier
  metadata?: Record<string, string>; // opaque host-owned key/value
};
```

#### `agent.heartbeat`

Correlated response to `server.heartbeat`. Must use the heartbeat
message's `messageId` as `correlationId`.

```ts
type AgentHeartbeat = {
  uptimeSeconds: number; // non-negative integer
};
```

#### `agent.printer_inventory`

Complete snapshot of all local printers. Optionally correlated to
`server.request_printer_inventory`.

```ts
type PrinterInventory = {
  revision: number; // monotonically increasing integer
  printers: PrinterDescriptor[];
};
```

#### `agent.printer_inventory_changed`

Incremental update. Only send the diff from the last revision.

```ts
type PrinterInventoryChanged = {
  revision: number;
  added: PrinterDescriptor[];
  updated: PrinterDescriptor[];
  removedPrinterIds: string[];
};
```

#### `agent.job_received`

Durable-persistence acknowledgement. The agent has written the job to
stable storage and will survive a restart.

```ts
type JobReceived = {
  jobId: string;
  idempotencyKey: string; // echo from PrintJob
  status: 'received';
  receivedAt: string; // ISO 8601 UTC
};
```

#### `agent.job_submitted`

The operating system or printer backend accepted the job. **Does not
mean paper was produced.**

```ts
type JobSubmitted = {
  jobId: string;
  idempotencyKey: string;
  printerId: string; // which printer accepted it
  status: 'submitted';
  submittedAt: string;
};
```

#### `agent.job_failed`

Recoverable or terminal failure.

```ts
type JobFailed = {
  jobId: string;
  idempotencyKey: string;
  status: 'failed';
  failedAt: string;
  error: {
    code: string; // stable kebab-case error code
    message: string; // human-readable, max 1 024 chars
    retryable: boolean;
  };
};
```

#### `agent.diagnostics`

Bounded, sanitized operational summary. Safe to store and display.

```ts
type AgentDiagnostics = {
  agentId: string;
  collectedAt: string;
  health: 'healthy' | 'degraded' | 'unhealthy';
  queueDepth: number;
  printersOnline: number;
  printersTotal: number;
  issues: Array<{
    code: string;
    message: string; // max 1 024 chars
    severity: 'info' | 'warning' | 'error';
  }>; // max 64 entries
};
```

---

### Server → Agent messages

#### `server.hello`

Correlated handshake response. Confirms version negotiation.

```ts
type ServerHello = {
  serverId: string;
  serverVersion: string;
  brand: { name: string }; // display identity, no external URLs
  sessionId: string;
  supportedProtocolVersions: number[];
  selectedProtocolVersion: number; // must be 1 currently
  heartbeatIntervalMs: number; // 5 000 – 300 000
  maxMessageBytes: number; // ≤ 2 097 152
};
```

#### `server.heartbeat`

Liveness probe. Agent must reply with `agent.heartbeat` using this
message's `messageId` as `correlationId`.

```ts
type HeartbeatRequest = {
  timeoutMs: number; // 1 000 – 120 000; agent must respond within this window
};
```

#### `server.print_job`

At-least-once job delivery. The `payload` is a `PrintJob`.

```ts
type PrintJob = {
  jobId: string; // stable UUID
  idempotencyKey: string; // server-assigned; agent uses this for dedup
  printerId: string; // must match a registered local printer id
  createdAt: string;
  document: PrintDocument;
  metadata?: Record<string, string>; // opaque, max 32 entries
};
```

#### `server.cancel_job`

Cancel a job not yet submitted by the agent.

```ts
type CancelJob = {
  jobId: string;
  reason?: string; // max 512 chars
};
```

#### `server.request_printer_inventory`

Request a full inventory snapshot. Payload is empty `{}`.

#### `server.configuration_invalidated`

Tells the agent that host-owned config has changed.

```ts
type ConfigurationInvalidated = {
  scope: 'agent' | 'printers' | 'all';
  revision?: string; // opaque version tag
};
```

#### `server.disconnect`

Explains an intentional disconnect and reconnection policy.

```ts
type Disconnect = {
  code: string; // stable identifier
  reason: string; // max 1 024 chars
  reconnect: boolean;
  retryAfterMs?: number; // 0 – 86 400 000
};
```

---

## Document model (`PrintDocument`)

A `PrintDocument` is the payload in `server.print_job`. It is
printer-independent structured content — rendering and physical
submission happen outside this package.

```ts
type PrintDocument = {
  width: 58 | 80; // receipt paper width in mm
  sections: PrintSection[]; // 1 – 256 entries
};
```

### Section types

| `type`    | Key fields                                                          |
| --------- | ------------------------------------------------------------------- |
| `text`    | `value: string` (max 16 384), optional `align: 'left'               | 'center'                                          | 'right'`, optional `bold: boolean` |
| `row`     | `left: string`, `right: string` (two-column layout, max 4 096 each) |
| `divider` | _(no extra fields)_                                                 |
| `image`   | `mediaType: 'image/png'                                             | 'image/jpeg'`, `data: string` (base64, max ~1 MB) |
| `qr`      | `value: string` (1 – 4 096 chars)                                   |
| `barcode` | `format: 'code128'                                                  | 'code39'                                          | 'ean13'                            | 'upca'`, `value: string` (printable ASCII, max 256) |
| `feed`    | `lines: integer` (1 – 255)                                          |
| `cut`     | _(no extra fields)_                                                 |

---

## Printer descriptor (`PrinterDescriptor`)

```ts
type PrinterDescriptor =
  | { kind: 'local';   connection: { type: 'system'; systemName: string }; ...fields }
  | { kind: 'network'; connection: { type: 'tcp'; host: string; port: number }; ...fields }
  | { kind: 'virtual'; connection: { type: 'virtual' }; ...fields };

// Common fields across all kinds:
// id: string
// fingerprint: string
// name: string
// capabilities?: PrinterCapabilities
// enabled: boolean
// availability: 'online' | 'offline' | 'unknown'
```

```ts
type PrinterCapabilities = {
  mediaWidths: (58 | 80)[];
  raster: boolean;
  cut: boolean;
  qr: boolean;
  barcode: boolean;
};
```

---

## Codec API

All functions validate with TypeBox and throw structured errors if
validation fails.

### Parse (validates an already-parsed JavaScript value)

```ts
import {
  parseAgentMessage,
  parseServerMessage,
  parseProtocolMessage,
  parsePrintDocument,
  parsePrintJob,
  parsePrinterDescriptor,
} from '@openprinter/protocol';

const msg = parseAgentMessage(JSON.parse(rawString));
```

### Decode (validates UTF-8 JSON string or `Uint8Array`)

```ts
import {
  decodeAgentMessage,
  decodeServerMessage,
  decodeProtocolMessage,
} from '@openprinter/protocol';

const msg = decodeAgentMessage(wsEvent.data); // string | Uint8Array
```

Decode enforces the 2 MB wire limit **before** parsing the JSON.

### Encode (validates + serializes to compact JSON string)

```ts
import {
  encodeAgentMessage,
  encodeServerMessage,
  encodeProtocolMessage,
} from '@openprinter/protocol';

const wire = encodeServerMessage({
  protocolVersion: 1,
  messageId: crypto.randomUUID(),
  sentAt: new Date().toISOString(),
  type: 'server.print_job',
  payload: job,
});
ws.send(wire);
```

---

## Error classes

```ts
import {
  ProtocolError,
  ProtocolValidationError,
  UnsupportedProtocolVersionError,
} from '@openprinter/protocol';

// Base class for all protocol errors
class ProtocolError extends Error {
  readonly code: string; // e.g. 'message_too_large', 'invalid_json'
}

// Thrown when TypeBox validation fails
class ProtocolValidationError extends ProtocolError {
  readonly issues: ProtocolIssue[]; // [{ path: string; message: string }]
}

// Thrown when the peer uses an unsupported protocolVersion
class UnsupportedProtocolVersionError extends ProtocolError {
  readonly receivedVersion: unknown;
}
```

---

## Constants

```ts
import {
  PROTOCOL_VERSION, // 1
  PROTOCOL_SCHEMA_ID, // 'https://openprinter.dev/schema/openprinter-v1.schema.json'
  MAX_WIRE_MESSAGE_BYTES, // 2 097 152 (2 MB)
  MAX_DOCUMENT_SECTIONS, // 256
  MAX_PRINTERS_PER_INVENTORY, // 512
  MAX_IMAGE_BASE64_LENGTH, // 1 398 104 (~1 MB)
  MAX_METADATA_ENTRIES, // 32
  PROTOCOL_LIMITS, // frozen object with all of the above
  AGENT_MESSAGE_TYPES, // readonly array of agent type discriminators
  SERVER_MESSAGE_TYPES, // readonly array of server type discriminators
} from '@openprinter/protocol';
```

---

## TypeScript helper types

```ts
import type {
  AgentMessage,
  ServerMessage,
  AgentMessageOf,
  ServerMessageOf,
} from '@openprinter/protocol';

// Extract one variant by discriminator:
type JobReceived = AgentMessageOf<'agent.job_received'>;
type PrintJobMsg = ServerMessageOf<'server.print_job'>;

// Full unions:
type AnyAgentMessage = AgentMessage;
type AnyServerMessage = ServerMessage;
```

---

## Discriminating messages at runtime

```ts
const msg = decodeAgentMessage(raw);

switch (msg.type) {
  case 'agent.hello':
    /* msg.payload is AgentHello */ break;
  case 'agent.job_received':
    /* msg.payload is JobReceived */ break;
  case 'agent.job_submitted':
    /* msg.payload is JobSubmitted */ break;
  case 'agent.job_failed':
    /* msg.payload is JobFailed */ break;
  case 'agent.printer_inventory':
    /* msg.payload is PrinterInventory */ break;
  // ... etc.
}
```

---

## Important constraints

- Never use `'printed'` as a status — use `'received'`, `'submitted'`,
  or `'failed'`. `'submitted'` means the OS/printer accepted the job,
  **not** that paper was produced.
- Agent auth tokens belong in the transport handshake (e.g. WebSocket
  headers), not in `agent.authentication_metadata`.
- `idempotencyKey` is server-assigned and must be echoed verbatim in
  all job status messages.
- Images must be raw base64 (no `data:` URI prefix).
- `PrinterDescriptor.id` is the opaque string the server uses to route
  jobs. Validate it against known printers before sending a
  `server.print_job`.
- The `agent.hello` `agentId` must stay stable across reconnections.
  Sessions with the same `agentId` are independent; the server decides
  replacement policy.

---

## Protocol change rules

When modifying this package:

1. Update the canonical TypeBox schema and inferred types in
   `packages/protocol/src/`.
2. Regenerate the JSON Schema: `pnpm protocol:generate`.
3. Update valid and invalid fixtures in `protocol/`.
4. Pass both Rust and TypeScript compatibility tests.
5. Document delivery and error semantics.

Use stable discriminators. Do not rename existing `type` values.

---

## Workspace commands

```bash
pnpm --filter @openprinter/protocol build    # compile TypeScript → dist/
pnpm --filter @openprinter/protocol generate # regenerate JSON Schema
pnpm --filter @openprinter/protocol check    # typecheck + test
pnpm --filter @openprinter/protocol test     # vitest
```
