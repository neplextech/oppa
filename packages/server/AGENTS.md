# `@openprinter/server` contributor guide

This package is the framework-neutral, live-connection boundary
between a host application and OpenPrinter-compatible agents.

## Ownership

- Accept existing Node-compatible HTTP upgrade requests through `ws`.
- Authenticate Bearer tokens through a host callback.
- Validate and encode every wire message with `@openprinter/protocol`.
- Negotiate the protocol before exposing a session as connected.
- Track only live connections and their latest printer inventory.
- Route acknowledgement, submission, failure, diagnostics, and
  lifecycle messages through narrow typed callbacks.
- Enforce handshake, heartbeat, and message-size limits.

## Boundaries

- Do not add a database, durable queue, retry scheduler, tenant model,
  user session, or printer-routing policy.
- Do not prescribe JWT or any other access-token format.
- Do not duplicate protocol schemas or wire types in this package.
- A newly authenticated connection replaces an older connection with
  the same agent ID.
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

Tests use real loopback HTTP/WebSocket connections where behavior
crosses the upgrade boundary. Keep time-based tests deterministic and
fast.
