# OPPA and OpenPrinter website

This Next.js application is the OpenPrinter landing page and the canonical human-facing
documentation for OPPA, the protocol, and the server SDK. Fumadocs MDX supplies typed content,
navigation, page rendering, and search.

## Responsibilities

- distinguish the OPPA agent from the OpenPrinter ecosystem
- document architecture and security boundaries
- provide executable local and server integration guides
- describe protocol, delivery, and idempotency semantics

It does not host an agent gateway, authorization service, or printer dashboard.

## Development

From the repository root:

```bash
pnpm install
pnpm docs:dev
pnpm --filter @oppa/www build
```

Documentation lives in `content/docs`. Keep examples aligned with the shared protocol fixtures and
avoid claims about printer support that is not implemented and tested.
