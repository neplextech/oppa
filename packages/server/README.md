# `@openprinter/server`

`@openprinter/server` attaches OpenPrinter agent connections to an existing Node-compatible HTTP
server. It authenticates agents, performs the protocol handshake, validates messages, tracks live
printer inventory, and exposes typed lifecycle callbacks.

It deliberately does not provide a database, durable queue, retry scheduler, authorization policy,
or application-specific printer routing. The integrating application retains a print job until it
observes the agent's `received` acknowledgement.

## Install

```bash
pnpm add @openprinter/server @openprinter/protocol
```

The package targets Node.js 20 or newer, Bun, and Deno's Node compatibility layer.

## Attach to an HTTP server

```ts
import { createServer } from "node:http";
import { createOpenPrinterServer } from "@openprinter/server";

const httpServer = createServer();
const openPrinter = createOpenPrinterServer({
  path: "/openprinter/agent",
  authenticateAgent: async ({ token }) => {
    const agent = await lookupOpaqueToken(token);
    return agent
      ? {
          agentId: agent.id,
          metadata: { organizationId: agent.organizationId },
        }
      : null;
  },
  onAgentConnected: ({ agent }) => {
    console.info("Agent connected", agent.agentId);
  },
  onJobReceived: ({ agent, message }) => {
    // Mark the application's durable job as received.
    console.info("Job received", agent.agentId, message.payload.jobId);
  },
  onJobSubmitted: ({ agent, message }) => {
    // Submitted means accepted by the printer backend, not physically printed.
    console.info("Job submitted", agent.agentId, message.payload.jobId);
  },
  onJobFailed: ({ agent, message }) => {
    console.error(
      "Job failed",
      agent.agentId,
      message.payload.jobId,
      message.payload.error,
    );
  },
});

httpServer.on("upgrade", openPrinter.handleUpgrade);
httpServer.listen(8787);
```

`authenticateAgent` receives the Bearer token and original upgrade request. Return `null` to reject
it. The SDK neither interprets nor logs the token.

## Deliver a job

```ts
const result = await openPrinter.sendJob("agent_123", {
  jobId: "job_123",
  idempotencyKey: "invoice_123_v1",
  printerId: "printer_123",
  createdAt: new Date().toISOString(),
  document: {
    width: 80,
    sections: [
      {
        type: "text",
        value: "Test receipt",
        align: "center",
        bold: true,
      },
      { type: "cut" },
    ],
  },
});

if (!result.ok) {
  // Keep or queue the job in application-owned durable storage.
  console.error(result.reason, result.retryable);
}
```

`send` and `sendJob` return discriminated results. They do not silently queue jobs. A disconnected
agent returns `agent-offline`; a connection that closes during delivery returns `connection-closed`.

## Live state

- `listAgents()` returns immutable snapshots of authenticated, handshaken sessions.
- `getAgent(agentId)` returns one session snapshot.
- `getPrinters(agentId)` returns the most recent validated inventory.
- `requestPrinters(agentId)` asks an online agent for a fresh snapshot.
- `disconnect(agentId)` closes a live session.
- `close()` stops accepting upgrades and closes all sessions.

State is in memory and disappears when the process exits. A second connection for the same
authenticated agent replaces the first.

Inventory revisions must advance within a session. An identical full snapshot may repeat its current
revision, but a changed snapshot or incremental update must use a newer revision; structurally
inconsistent changes close only the offending connection.

Job callbacks contain runtime-validated agent messages, not authorization to mutate arbitrary host
records. The host must match the correlation, job, idempotency, and agent identifiers against its
own durable job. The SDK cannot safely preserve that application-owned correlation state across
process restarts or agent reconnects.

## Limits and lifecycle

Defaults are intentionally bounded:

- 2 MiB maximum WebSocket message
- 10 second protocol handshake timeout
- 15 second heartbeat interval
- 45 second heartbeat timeout
- 10 second authentication timeout
- 5 second lifecycle callback timeout

All values are configurable. Invalid payloads and handshake violations invoke `onProtocolError` and
close only the offending connection. Callback failures and timeouts are reported through
`onCallbackError` and do not become protocol failures or hold a connection open indefinitely.

## Development

```bash
pnpm --filter @openprinter/server check
pnpm --filter @openprinter/server test
pnpm --filter @openprinter/server build
```

See `examples/node-server` for a complete development-only authorization flow and HTTP API.
