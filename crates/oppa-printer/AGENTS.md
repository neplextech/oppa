# oppa-printer contributor guidance

This crate owns printer-domain types, not platform I/O or product
routing.

- Preserve the distinction between an OPPA-assigned `PrinterId` and
  mutable fingerprint evidence.
- Do not make MAC addresses mandatory or universally unique.
- Keep submission semantics at “accepted/submitted”; do not imply that
  physical printing completed.
- Add new connection variants only with validation, stable serde
  names, tests, and an implemented consumer.
- Avoid dependencies on renderer, spooler, storage, protocol, Tauri,
  or UI crates. Submission contracts that require rendered types
  belong in `oppa-spooler`.
- Document every public item and test invalid provider/user input.

Run `cargo test -p oppa-printer`, `cargo fmt --check`, and Clippy
before handing off changes.
