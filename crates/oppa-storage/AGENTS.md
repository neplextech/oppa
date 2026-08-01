# oppa-storage contributor guidance

This crate owns durable, non-secret local agent state.

- Preserve the invariant: commit a validated job before the agent
  acknowledges receipt.
- Keep migrations append-only once released. Add the next numbered SQL
  file and migration entry; never rewrite a deployed migration.
- Do not store private keys, pairing codes, signatures, complete
  challenges, or other secret credentials in SQLite. Paired public
  identifiers and opaque credential references are non-secret
  metadata.
- Keep job state transitions explicit and transactional. `submitted`
  means a backend accepted the job, not that physical output was
  verified.
- Bound queue growth, JSON sizes, diagnostic messages, and database
  wait times.
- Treat duplicate job IDs and idempotency keys as expected
  at-least-once delivery behavior.
- Protocol-to-storage conversion belongs in `oppa-agent`; do not
  couple SQL migrations to wire structs.
- Test migrations, duplicate handling, every new transition, and
  restart recovery on a file-backed temporary database.

Run `cargo test -p oppa-storage`, `cargo fmt --check`, and Clippy
before handing off changes.
