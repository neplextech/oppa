# `@openprinter/server` contributor guide

This package is the framework-neutral protocol-session boundary
between a host application and OpenPrinter-compatible agents.

## Ownership

- Accept identities that the host has already authenticated.
- Run over host-supplied `send` and `close` transport callbacks.
- Validate and encode every wire message with `@openprinter/protocol`.
- Negotiate the protocol before exposing a session as connected.
- Track only per-session protocol state and latest printer inventory.
- Route acknowledgement, submission, failure, diagnostics, and
  lifecycle messages through narrow typed callbacks.
- Enforce handshake, heartbeat, and message-size limits.

## Boundaries

- Do not add HTTP/WebSocket ownership, authentication parsing, a
  global connection registry, cluster coordination, a broker,
  database, durable queue, retry scheduler, tenant model, user
  session, or printer-routing policy.
- Do not prescribe JWT or any other access-token format.
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
