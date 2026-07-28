# oppa-core

`oppa-core` contains the small set of domain primitives shared across the OPPA
Rust workspace: validated identifiers, UTC timestamps, agent and job lifecycle
states, and diagnostic error categories.

## Responsibilities

- Give agent, printer, job, and product identifiers distinct Rust types.
- Keep durable status terminology (`received`, `submitted`, and `failed`)
  consistent across infrastructure crates.
- Provide types that serialize predictably without importing protocol or
  infrastructure concerns.

It intentionally does not perform I/O, persistence, protocol validation,
rendering, discovery, or platform integration. Higher-level crates should add
their own detailed error types and use `ErrorCategory` only when a coarse,
sanitized classification is useful.

## Primary APIs

- `AgentId`, `PrinterId`, `PrintJobId`, and `ProductId`
- `Timestamp`
- `AgentState` and `JobState`
- `ErrorCategory`

This is a leaf crate in the Rust dependency graph.

## Development

From the repository root:

```bash
cargo test -p oppa-core
cargo clippy -p oppa-core --all-targets -- -D warnings
```

The current implementation is production-ready foundation code; domain types
will grow only when more than one crate genuinely shares them.
