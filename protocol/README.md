# OpenPrinter protocol artifacts

This directory contains language-neutral artifacts shared by the canonical TypeBox implementation in
`packages/protocol` and the Rust mirror in `crates/oppa-protocol`.

- `schema/openprinter.schema.json` is generated deterministically from TypeBox.
- `fixtures/agent` contains valid agent-to-server messages.
- `fixtures/server` contains valid server-to-agent messages.
- `fixtures/invalid` contains payloads every implementation must reject.

Do not hand-edit the generated schema. Run `pnpm --filter @openprinter/protocol schema:generate`,
then run both protocol test suites whenever the wire contract changes.
