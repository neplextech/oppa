# `@openprinter/server` contributor guide

This package is the framework-neutral protocol-session boundary
between a host application and OpenPrinter-compatible agents.

## Ownership

- Expose discovery and one-time pairing over host-owned HTTP routes.
- Authenticate accepted transports against paired Ed25519 public
  credentials.
- Run over host-supplied `send` and `close` transport callbacks.
- Validate and encode every wire message with `@openprinter/protocol`.
- Negotiate the protocol before exposing a session as connected.
- Track only per-session protocol state and latest printer inventory.
- Route acknowledgement, submission, failure, diagnostics, and
  lifecycle messages through narrow typed callbacks.
- Enforce handshake, heartbeat, and message-size limits.

## Boundaries

- Do not add HTTP/WebSocket listener ownership, application login
  policy, a global connection registry, cluster coordination, a
  broker, database, durable queue, retry scheduler, tenant model, user
  session, or printer-routing policy.
- Keep private keys out of the SDK. Persist only public credentials
  through the host-provided store.
- Pairing codes are single-use and atomically consumed; core code
  never logs them.
- Do not duplicate protocol schemas or wire types in this package.
- Sessions with the same agent ID remain independent; the host decides
  routing and replacement policy.
- `received`, `submitted`, and `failed` are distinct states. Never
  infer physical printing from `submitted`.
- Public APIs require useful JSDoc and errors/results require stable
  discriminators.

## Development

```bash
pnpm --filter @openprinter/server check
pnpm --filter @openprinter/server test
pnpm --filter @openprinter/server build
```

Tests use in-memory host transports. Keep receive-order and time-based
tests deterministic and fast.
