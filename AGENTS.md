# OPPA workspace guidance

## Purpose and terminology

OPPA is the **Open Printer Proxy Agent**, a local Tauri desktop
utility backed by a shell-independent Rust runtime. OpenPrinter is the
generic protocol and server SDK ecosystem in the `@openprinter/*` npm
scope.

Keep product-specific concepts such as restaurant, branch, kitchen,
billing, menu, order, tenant routing, and hosted dashboards out of
this repository's generic protocol and agent layers.

## Workspace map

- `apps/oppa`: React interface and narrow Tauri host
- `apps/www`: Fumadocs landing and documentation site
- `crates/oppa-agent`: top-level Rust orchestration
- `crates/oppa-*`: focused domain and infrastructure boundaries
- `packages/protocol`: canonical TypeBox protocol schemas, codecs, and
  types
- `packages/server`: framework-neutral authenticated WebSocket SDK
- `protocol`: generated JSON Schema and cross-language fixtures
- `examples/node-server`: development-only end-to-end integration
- `products`: compile-time branded product definitions and assets

Do not add a crate, package, application, or service unless its
responsibility cannot be represented cleanly by an existing unit and
is required by `PLAN.md`.

## Dependency direction

`apps/oppa` hosts `oppa-agent`; it does not own agent business logic.
`oppa-agent` coordinates authentication, transport, storage,
discovery, rendering, spoolers, platform services, and the lower-level
domain crates. Low-level crates must not depend on Tauri, frontend
code, or `oppa-agent`. Avoid circular dependencies.

Rendering and printer submission are separate. Credentials use
`oppa-platform` secure storage and must never enter SQLite. The server
SDK never owns durable server-side jobs.

## Protocol source of truth

The TypeBox schema in `packages/protocol` is canonical. Its
deterministic generator writes
`protocol/schema/openprinter.schema.json`. Rust and TypeScript
validate the same committed fixtures.

A protocol change must:

1. update the canonical schema and inferred types;
2. regenerate the JSON Schema;
3. update valid and invalid fixtures;
4. pass both Rust and TypeScript compatibility tests;
5. document delivery and error semantics.

Use stable discriminators and serialized names. Prefer `received`,
`submitted`, and `failed`; do not claim universal physical `printed`
completion.

## Product configuration

`OPPA_PRODUCT_DIR` selects a versioned `product.json` and assets at
compile time. Runtime code uses the validated embedded definition and
never trusts an editable product file. Product configuration may
disable a compiled capability but cannot enable code absent from the
binary.

## Security constraints

- no arbitrary shell commands, scripts, plugin execution, or generic
  proxying
- no unrestricted frontend filesystem, SQL, shell, or network
  capability
- discovery and pairing accept plain HTTP only on loopback; production
  service and gateway endpoints require TLS
- private Ed25519 keys remain behind `oppa-platform` secure storage;
  only public keys cross the network
- gateway challenges are unpredictable, socket-bound, single-use, and
  expire before normal protocol traffic is accepted
- credentials stay in operating-system secure storage and out of logs
  and diagnostics
- validate message, document, image, queue, and diagnostic limits
- validate a remote printer ID against the enabled local registry
- apply explicit timeouts to all printer and network operations
- sanitize errors and never log full print documents by default

## Code and documentation

Use TypeScript strict mode and typed error surfaces. Avoid `any`,
unchecked parsing, hidden global state, giant files, and speculative
abstractions. Rust public APIs need useful Rustdoc; TypeScript public
APIs need JSDoc. Recoverable failures return structured errors and
must not panic.

Every crate, package, application, and example needs a professional
README describing purpose, responsibilities, non-responsibilities,
APIs, dependency role, commands, and current status.

`CLAUDE.md` files only contain `@AGENTS.md`; do not duplicate guidance
between them.

## Commands

```bash
pnpm install
pnpm format:check
pnpm lint
pnpm typecheck
pnpm test
pnpm build
pnpm protocol:generate
```

Targeted Rust checks:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Physical printers are not required in tests. Use virtual printers and
mocks. Keep static/build validation distinct from a real Tauri launch
or physical printer acceptance test.

## Non-goals

Do not add a hosted OpenPrinter cloud, built-in Redis or database
server, browser printing, remote desktop, arbitrary runtime provider
switching, mobile application, business fleet dashboard, full QZ Tray
compatibility, or speculative hardware protocols.
