# `@openprinter/server`

Framework-neutral discovery, pairing, gateway authentication, and protocol-session SDK for OpenPrinter agents.

The SDK owns discovery documents, one-time pairing grants, Ed25519 public credentials, challenge verification, the OpenPrinter hello exchange, heartbeat, printer inventory, commands, and typed lifecycle callbacks. The host still owns its HTTP/WebSocket framework, durable credential persistence, rate-limit policy, live-session routing, cluster coordination, and durable jobs.

## Install

```bash
pnpm add @openprinter/server @openprinter/protocol
```

## Create a server

```ts
import { createOpenPrinterServer } from '@openprinter/server';

const openprinter = createOpenPrinterServer({
  brand: { name: 'Acme Print Service' },
  serverId: 'acme-print-v1',
  serverVersion: '1.0.0',
  onAgentConnected: ({ agent, session }) => registerLiveSession(agent.agentId, session),
  onAgentDisconnected: ({ agent, session }) => removeLiveSessionIfCurrent(agent.agentId, session),
  onJobReceived: ({ agent, message }) => markReceived(agent.agentId, message.payload.jobId),
  onJobSubmitted: ({ agent, message }) => markSubmitted(agent.agentId, message.payload.jobId),
});
```

The default paths are available on `openprinter.paths`:

- `/.well-known/openprinter` for discovery
- `/openprinter/pair` for pairing
- `/.well-known/openprinter/gateway` for the agent WebSocket

Override them with `paths` when embedding the SDK into an existing application. All three paths must be distinct absolute URL paths.

## HTTP integration

Expose discovery and pairing with the framework of your choice:

```ts
app.get(openprinter.paths.discovery, async (_request, response) => {
  response.json(await openprinter.discover());
});

app.post(openprinter.paths.pairing, async (request, response) => {
  response.json(await openprinter.pair(request.body, {
    remoteAddress: request.ip,
  }));
});
```

Pairing codes are cryptographically random, human-readable, case-insensitive, short-lived, single-use, and atomically consumed:

```ts
const pairing = await openprinter.createPairingCode({
  expiresInMs: 5 * 60_000,
  metadata: { organizationId: 'org_123' },
});

showPairingCodeToOperator(pairing.code);
```

The SDK never logs the code. `checkPairingRateLimit` provides the policy hook for limiting attempts. The included `InMemoryPairingCodeStore` and `InMemoryAgentCredentialStore` are suitable for tests and local examples only; production hosts must supply durable implementations of `PairingCodeStore` and `AgentCredentialStore`.

## Gateway integration

For a `ws`-compatible socket, use the convenience adapter:

```ts
httpServer.on('upgrade', (request, socket, head) => {
  if (request.url !== openprinter.paths.gateway) return socket.destroy();
  wss.handleUpgrade(request, socket, head, (webSocket) => {
    openprinter.handleGatewayConnection(webSocket);
  });
});
```

For any other transport, attach its lifecycle to the small session boundary:

```ts
const session = openprinter.accept({
  transport: {
    send: (message) => connection.send(message),
    close: ({ reason, detail }) => connection.close(mapCloseReason(reason), detail),
  },
});

connection.onMessage((message) => void session.receive(message));
connection.onClose(() => void session.transportClosed());
```

`accept()` starts unauthenticated. The SDK sends a socket-bound, expiring challenge, looks up the claimed `(agentId, keyId)`, verifies the Ed25519 signature, sends `auth.accepted`, and only then accepts `agent.hello`. The challenge is unpredictable and single-use. Normal protocol frames are protected by the authenticated TLS/WebSocket session and are not signed individually.

## Deliver jobs

The host selects a connected session from its own registry or backplane and calls `sendJob`:

```ts
const result = await session.sendJob({
  jobId: 'job_123',
  idempotencyKey: 'invoice_123_v1',
  printerId: 'printer_123',
  createdAt: new Date().toISOString(),
  document: { width: 80, sections: [{ type: 'text', value: 'Receipt' }] },
});
```

`sendJob` handing a frame to a live transport is not durable server storage. Keep the job in application-owned storage until the agent acknowledgement and apply application retry policy outside this SDK.

## Credential revocation and failures

Call `revokeCredential(agentId, keyId)` to revoke a paired public credential. Future authentication attempts receive a bounded machine-readable rejection and the gateway closes. Discovery, pairing, authentication, handshake, transport, callback, heartbeat, and message sizes all have explicit validation or timeouts.

## Development

```bash
pnpm --filter @openprinter/server check
pnpm --filter @openprinter/server test
pnpm --filter @openprinter/server build
```

See `examples/node-server` for a complete HTTP and WebSocket integration.
