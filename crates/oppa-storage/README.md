# oppa-storage

`oppa-storage` is OPPA's durable SQLite boundary. It persists validated jobs before receipt
acknowledgement, enforces idempotency, models legal state transitions, and restores interrupted
submissions after restart.

## Responsibilities

- Apply embedded, ordered database migrations.
- Bound the non-terminal queue, terminal history, outbox pages, and stored JSON sizes.
- Distinguish duplicate job IDs from duplicate idempotency keys and reject a same-ID/key redelivery
  whose printer or print-job content changed.
- Atomically claim a job before submission.
- Atomically persist `submitted` or `failed` outcomes with their transport status in a durable
  outbox.
- Prune terminal print payloads while retaining their SHA-256 fingerprints for exact-redelivery
  checks.
- Store configured printers, bindings, non-secret settings, connection metadata, and sanitized
  diagnostics metadata.

Access and refresh credentials are intentionally absent from every migration; they belong in
`oppa-platform` secure credential storage. The repository stores an already validated JSON payload
to keep OpenPrinter schema evolution isolated from SQL; `oppa-agent` owns the narrow protocol
adapter.

`StorageLimits` defaults to 1,000 non-terminal jobs and 10,000 terminal jobs. Terminal maintenance
runs after each terminal transition and at database open. It replaces terminal payload bodies with
JSON `null`, keeps the content fingerprint and bounded result metadata, and deletes the oldest rows
beyond the configured count. Outbox rows are foreign-keyed to terminal jobs, so an unacknowledged
status has the same finite retention horizon instead of growing without bound. Hosts that need a
longer offline replay window must configure a larger terminal limit.

`mark_submitted_with_status` and `mark_failed_with_status` commit the lifecycle transition and
outbound report in one SQLite transaction. The agent reads outbox rows oldest first and removes each
row only after transport delivery succeeds. Delivery across the final transport-send/database-ack
boundary is therefore at least once.

## Primary APIs

- `SqliteStorage`
- `JobRepository`
- `StorageLimits`, `ReceivedPrintJob`, and `StoredPrintJob`
- `InsertResult`, `StoredJobError`, and `RecoveryResult`
- `NewOutboundStatus`, `StoredOutboundStatus`, and `TerminalRetentionResult`

## Development

```bash
cargo test -p oppa-storage
cargo clippy -p oppa-storage --all-targets -- -D warnings
```

SQLite is bundled for reproducible desktop builds. Tests use private temporary databases and require
no external service.
