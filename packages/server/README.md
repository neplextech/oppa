# `@openprinter/server`

`@openprinter/server` is the framework-neutral protocol-session SDK for
OpenPrinter-compatible agents. A host application authenticates a connection,
supplies a small `send`/`close` transport, and forwards inbound frames. The SDK
then owns the OpenPrinter handshake, validation, heartbeat, printer inventory,
commands, and typed lifecycle events for that one session.

The package does not create an HTTP or WebSocket server, parse credentials, keep
a global agent registry, coordinate Node cluster workers, or prescribe a
message broker. It also does not provide durable jobs, retries, application
authorization, or printer-routing policy.

## Install

```bash
pnpm add @openprinter/server @openprinter/protocol
```

Install the transport used by your host separately, such as `ws`, a
framework-native WebSocket adapter, or a broker client.

## Create the protocol server

```ts
import { createOpenPrinterServer } from '@openprinter/server';

const openPrinter = createOpenPrinterServer({
  brand: {
    name: 'Acme POS',
  },
  serverId: 'acme-openprinter',
  serverVersion: '1.0.0',

  onAgentConnected: ({ agent, session }) => {
    // Store `session` in a registry/backplane owned by your application.
    registerLiveSession(agent.agentId, session);
  },

  onAgentDisconnected: ({ agent, session, reason }) => {
    removeLiveSessionIfCurrent(agent.agentId, session);
    console.info('Agent disconnected', agent.agentId, reason);
  },

  onJobReceived: ({ agent, message }) => {
    // The agent has durably persisted the job.
    markJobReceived(agent.agentId, message.payload.jobId);
  },

  onJobSubmitted: ({ agent, message }) => {
    // Submitted means backend acceptance, not verified physical printing.
    markJobSubmitted(agent.agentId, message.payload.jobId);
  },

  onJobFailed: ({ agent, message }) => {
    markJobFailed(agent.agentId, message.payload);
  },
});
```

`brand.name` is required and is sent in `server.hello` so OPPA can show which
service it connected to. Brand metadata intentionally has no icon or external
resource URL.

## Accept a host-owned transport

Authentication happens before `accept()`. The authenticated identity is
authoritative and must match the subsequent `agent.hello`.

```ts
const identity = await authenticateConnection(request);

if (identity === null) {
  rejectConnection();
  return;
}

const session = openPrinter.accept({
  identity: {
    agentId: identity.agentId,
    metadata: {
      organizationId: identity.organizationId,
    },
  },
  transport: {
    send: (message) => connection.send(message),
    close: ({ reason, detail }) => {
      connection.close(mapTransportClose(reason), detail);
    },
  },
});

connection.onMessage((message) => {
  void session.receive(message);
});

connection.onClose((detail) => {
  void session.transportClosed({
    reason: 'peer-closed',
    detail,
  });
});

connection.onError((error) => {
  void session.transportClosed({
    reason: 'transport-error',
    detail: safeErrorName(error),
  });
});
```

The transport contract is deliberately small:

- `send(message)` accepts one encoded UTF-8 JSON protocol message. Resolving
  means immediate handoff to the transport, not agent persistence or printing.
- `close(request)` lets the host map a stable OpenPrinter reason to its own
  socket, consumer, route, or connection lifecycle.
- `receive(message)` accepts `string` or `Uint8Array` input and serializes
  concurrent calls in invocation order.
- `transportClosed(event)` tells the protocol session that host-owned
  connectivity has already ended; it does not call `close` again.

## Deliver jobs through a selected session

The host selects a session using its own local registry or distributed
backplane:

```ts
const session = await resolveAgentSession('agent_123');

if (session === null) {
  // Keep the job in application-owned durable storage.
  return {
    ok: false,
    agentId: 'agent_123',
    reason: 'agent-offline',
    retryable: true,
  };
}

const result = await session.sendJob({
  jobId: 'job_123',
  idempotencyKey: 'invoice_123_v1',
  printerId: 'printer_123',
  createdAt: new Date().toISOString(),
  document: {
    width: 80,
    sections: [
      {
        type: 'text',
        value: 'Test receipt',
        align: 'center',
        bold: true,
      },
      { type: 'cut' },
    ],
  },
});

if (!result.ok) {
  // `session-not-ready`, `connection-closed`, or `transport-error`.
  retainForApplicationRetry(result);
}
```

Other session-local commands are:

- `send(message)`
- `requestPrinters()`
- `cancelJob(cancellation)`
- `invalidateConfiguration(invalidation)`
- `disconnect(options)`

`getAgent()` returns the negotiated agent snapshot and `getPrinters()` returns
that session's latest validated inventory.

## Cluster and broker ownership

Two accepted sessions with the same agent ID remain independent. The SDK does
not replace one, choose a process, or pretend a process-local map is
cluster-wide.

In a clustered application, the host can announce which worker owns a live
transport and route application commands to that worker through RabbitMQ,
Redis, NATS, IPC, or another backplane. The host is responsible for ownership,
affinity, ordering beyond one session, deduplication of broker redelivery, and
stale-route expiry. Each ingress worker calls the same `accept`, `receive`, and
session command APIs.

## Limits and lifecycle

Defaults are intentionally bounded:

- 2 MiB maximum encoded protocol message
- 10 second protocol handshake timeout
- 15 second heartbeat interval
- 45 second heartbeat timeout
- 10 second transport callback timeout
- 5 second lifecycle callback timeout

Invalid messages and handshake violations invoke `onProtocolError`, send a
semantic disconnect when possible, and ask the host to close only that
transport. Lifecycle callback failures and timeouts are isolated through
`onCallbackError`. A rejected or timed-out transport handoff returns a stable
structured delivery failure and makes the session unavailable.

## Development

```bash
pnpm --filter @openprinter/server check
pnpm --filter @openprinter/server test
pnpm --filter @openprinter/server build
```

See `examples/node-server` for a complete host-owned `ws` adapter,
development-only authorization flow, explicit local session registry, and HTTP
API.
