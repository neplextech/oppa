# OpenPrinter protocol artifacts

This directory contains language-neutral artifacts shared by the canonical TypeBox implementation in
`packages/protocol` and the Rust mirror in `crates/oppa-protocol`.

- `schema/openprinter.schema.json` is generated deterministically from TypeBox.
- `fixtures/agent` contains valid agent-to-server messages.
- `fixtures/server` contains valid server-to-agent messages.
- `fixtures/invalid` contains payloads every implementation must reject.

Protocol version `"1"` is the base protocol. Version `"2"` adds PDF print jobs and printer
document capabilities. Handshake frames always use envelope version `"1"`. Other messages use the
lowest version their content needs, and receivers reject extended content under a `"1"` envelope.
The `agent-hello-extended-offer.json` and `server-hello-extended.json` fixtures show a negotiated
extended handshake. Earlier peers reject the extended offer, which is covered in the protocol tests.

Do not hand-edit the generated schema. Run `pnpm --filter @openprinter/protocol schema:generate`,
then run both protocol test suites whenever the wire contract changes.
