# Website guidance

`apps/www` is the Fumadocs landing and documentation application.

- Keep OPPA and OpenPrinter definitions visibly distinct.
- Treat `PLAN.md`, the canonical protocol schema, and public
  crate/package APIs as authoritative.
- Prefer executable examples and explicit ownership boundaries over
  marketing copy.
- Do not claim physical print completion; use received, submitted, and
  failed.
- Do not document a hardware provider as supported until its
  implementation and tests exist.
- Run `pnpm --filter @oppa/www typecheck` and
  `pnpm --filter @oppa/www build` after content or route changes.
- Do not add a hosted product dashboard, gateway, analytics tracker,
  or authentication service here.
