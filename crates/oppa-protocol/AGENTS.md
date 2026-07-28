# Rust protocol crate guidance

This crate mirrors the canonical TypeBox schemas in
`../../packages/protocol`. Never change serialized names or add
Rust-only wire fields. A protocol change must update TypeBox first,
regenerate the committed JSON Schema, mirror the new shape here, and
add or update shared fixtures.

All inbound bytes go through the `decode_*` functions. Keep Serde
payloads strict, validate envelope fields and semantic bounds, reject
unsupported versions explicitly, and enforce the byte limit before
parsing. Errors must not retain full print payloads or secrets.

Use received/submitted/failed terminology precisely. Submitted means
backend acceptance, not verified physical output. Preserve both job
IDs and idempotency keys for at-least-once delivery.

Public items require useful Rustdoc. Keep dependencies minimal and do
not depend on Tauri, transport, storage, renderer, or printer
backends.

```sh
cargo test -p oppa-protocol
cargo fmt --check -p oppa-protocol
cargo clippy -p oppa-protocol --all-targets -- -D warnings
```
