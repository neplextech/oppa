---
name: openprinter-server
description: >
  @openprinter/server discovery, pairing stores, Ed25519 gateway
  authentication, framework-neutral transports, session lifecycle,
  printer inventory, heartbeats, and job delivery.
---

# OpenPrinter server skill

Use this skill when building or changing an OpenPrinter server
integration with `@openprinter/server`.

## Boundary

The SDK owns:

- validated discovery metadata and configurable default paths
- cryptographically random one-time pairing code creation
- pairing request validation and atomic consumption
- public credential lookup and revocation boundaries
- socket-bound Ed25519 challenge verification
- hello/version negotiation, heartbeats, message validation, and
  per-session printer inventory
- typed lifecycle and job acknowledgement callbacks

The host owns:

- HTTP and WebSocket framework routing
- authorization around pairing-code creation
- durable implementations of pairing and credential stores
- pairing-attempt rate-limit policy
- live session registry, affinity, cluster coordination, and
  backplanes
- durable server-side jobs, retry scheduling, and printer routing

Do not move host ownership into the SDK. The SDK should attach to a
transport and use its `send`/`close` boundary.

## Create and expose

```ts
const server = createOpenPrinterServer({
  brand: { name: 'Acme Print Service' },
  serverId: 'acme-print-v1',
  serverVersion: '1.0.0',
  pairingCodeStore,
  credentialStore,
  checkPairingRateLimit: async ({ remoteAddress }) =>
    limiter.check(remoteAddress),
  onAgentConnected: ({ agent, session }) =>
    sessions.set(agent.agentId, session),
});

app.get(server.paths.discovery, async (_request, response) => {
  response.json(await server.discover());
});

app.post(server.paths.pairing, async (request, response) => {
  response.json(
    await server.pair(request.body, { remoteAddress: request.ip }),
  );
});
```

Defaults are `/.well-known/openprinter`, `/openprinter/pair`, and
`/.well-known/openprinter/gateway`. Overrides must be distinct safe
absolute paths.

## Pairing stores

Create codes through `createPairingCode({ expiresInMs, metadata })`.
Codes are secure random, human-readable, case-insensitive,
approximately five minutes by default, single-use, and atomically
consumed. Do not log them in core code or place them in URLs.

`PairingCodeStore.consume` wraps public credential registration so
concurrent redemptions cannot both succeed. If registration fails, a
correct store must not lose a still-valid grant.
`AgentCredentialStore` creates, finds, and revokes Ed25519 public
credentials by `(agentId, keyId)`.

The in-memory stores are only for tests and disposable local examples.
Production hosts need durable implementations and rate limiting.

## Gateway

For a `ws`-compatible socket:

```ts
wss.handleUpgrade(request, socket, head, (webSocket) => {
  server.handleGatewayConnection(webSocket);
});
```

For another transport:

```ts
const session = server.accept({
  transport: {
    send: (message) => connection.send(message),
    close: ({ reason, detail }) =>
      connection.close(mapReason(reason), detail),
  },
});

connection.onMessage((frame) => void session.receive(frame));
connection.onClose(() => void session.transportClosed());
```

`accept()` starts authentication immediately. The server sends a
domain-separated opaque challenge, consumes exactly one response,
resolves the public credential, rejects revoked or unknown keys, and
verifies Ed25519 before allowing `agent.hello`. Never accept ordinary
protocol frames before `auth.accepted`. Do not sign ordinary messages.

## Session delivery

After `onAgentConnected`, a session may call `sendJob`,
`requestPrinters`, `cancelJob`, `invalidateConfiguration`, or
`disconnect`. `sendJob` reports transport handoff only. Keep
application jobs durable until acknowledgements arrive.

Two sessions for the same agent remain independent. The host decides
replacement, routing, distributed ownership, and stale-route behavior.

## Security and testing

- Bound authentication and normal frame sizes separately.
- Apply authentication, challenge, handshake, heartbeat, transport,
  and callback timeouts.
- Sanitize close details and do not log keys, pairing codes,
  challenges, signatures, or print documents.
- Test invalid, expired, consumed, and concurrent pairing codes.
- Test invalid public keys, unknown/revoked credentials, wrong
  signatures, challenge expiry/reuse, success, and reconnect.
- Use virtual printers and in-memory stores; no physical device is
  required.

## Commands

```bash
pnpm --filter @openprinter/server build
pnpm --filter @openprinter/server test
pnpm --filter openprinter-node-example check
```
