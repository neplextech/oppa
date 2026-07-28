# `oppa-protocol`

Validated Rust representation of the OpenPrinter wire protocol used by OPPA's transport, storage,
renderer, and agent layers.

## Responsibilities

- mirror the canonical TypeBox contract from `packages/protocol`
- serialize stable camel-case fields and explicit message discriminators
- reject malformed input, unsupported versions, unknown fields, and size-limit violations
- model structured print documents, printer descriptors, and idempotent jobs
- validate every shared cross-language fixture

The crate does not authenticate transports, persist jobs, discover printers, render documents, or
submit bytes to hardware. It is dependency-low and does not depend on Tauri or platform crates.

## Primary APIs

`AgentMessage` and `ServerMessage` contain a validated envelope and a discriminated
`AgentMessageKind` or `ServerMessageKind`. Use `decode_agent_message` and `decode_server_message` at
inbound trust boundaries; they enforce the hard UTF-8 byte limit before allocation-heavy processing
and then call `Validate`.

`PrintDocument`, `PrintSection`, `PrinterDescriptor`, and `PrintJob` are the domain payload types
shared by downstream crates. A submitted job has only been accepted by a backend; it is not
universally known to be physically printed.

`PrinterDescriptor::capabilities` is optional. `None` represents capabilities that discovery could
not determine and must be preferred over guessed media widths or feature support.

## Source of truth and development

The TypeBox schemas in `../../packages/protocol/src/schemas` are canonical. Their deterministic JSON
Schema output is committed at `../../protocol/schema/openprinter.schema.json`. Rust and TypeScript
both validate all files under `../../protocol/fixtures`. Rust string bounds count UTF-16 code units
to match TypeBox's JavaScript runtime behavior, including supplementary Unicode characters.

```sh
cargo test -p oppa-protocol
cargo fmt --check -p oppa-protocol
cargo clippy -p oppa-protocol --all-targets -- -D warnings
```

The current crate implements protocol version 1 and rejects all other versions with an explicit
`UnsupportedProtocolVersion` error.
