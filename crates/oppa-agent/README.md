# oppa-agent

`oppa-agent` is OPPA's shell-independent runtime and state machine. It owns the durable print-job
boundary while leaving transport, printer configuration, and desktop integration behind explicit
traits.

The crate has no Tauri dependency, contains no frontend state, and does not own
integrating-application routing rules. A host supplies a `JobRepository`, `PrinterResolver`,
`SpoolerRegistry`, and `OutboundReporter`.

## Primary APIs

- `AgentBuilder` assembles the runtime from explicit infrastructure boundaries.
- `Agent` dispatches print and cancellation messages, drains pending work, and performs startup
  recovery.
- `JobProcessor` exposes the focused durable receipt/submission flow for hosts that need to
  coordinate reporting themselves.
- `Agent::submit_local_test` validates, renders, and spools host-owned test content without creating
  a server job or outbound status.
- `AgentHandle` exposes lifecycle snapshots through a watch channel and bounded, payload-free job
  events through `subscribe_job_events`.
- `StaticPrinterResolver` is a validated mutable printer catalog suitable for simple hosts and
  tests.

```rust,ignore
let agent = AgentBuilder::new()
    .repository(repository)
    .printer_resolver(printers)
    .spooler_registry(spoolers)
    .outbound_reporter(reporter)
    .build()?;

// After the transport connects and sends agent.hello:
agent.replay_outbound_reports().await?;
agent.recover().await?;
agent.handle_server_message(&server_message).await?;
```

## Durable job semantics

The runtime follows this order:

```text
validate
→ insert received row
→ report job_received
→ resolve enabled printer
→ render for the connection family
→ mark submitting
→ submit with cooperative cancellation
→ persist submitted, failed, or cancelled
→ report job_submitted or job_failed
```

An exact at-least-once redelivery is acknowledged from its existing durable row and never submitted
again. Reuse of an idempotency key by a different job returns
`ReceiveJobOutcome::DuplicateIdempotency` and is not acknowledged.

After every reconnect, a host must connect the transport, send `agent.hello`, call
`Agent::replay_outbound_reports`, and then call `Agent::recover` before entering its normal message
loop. This explicit order prevents buffered or durable application messages from preceding the
handshake.

`Agent::recover` first moves interrupted `submitting` rows back to `received`. It then re-sends
`agent.job_received` for every pending job before claiming any of them, preserving
receipt-before-processing even if the original connection failed immediately after the durable
insert. Pending jobs are processed in oldest-first order. The recovery count and replay events
remain visible through `AgentHandle`. If a recovered replay fails, its terminal report uses
`job.recovery_uncertain` and explicitly warns that the interrupted attempt may already have reached
its backend.

Cancellation is best effort. A queued job can become durably cancelled immediately. An active
spooler receives a `CancellationToken`; backend acceptance wins if it completed before cancellation
was observed.

Terminal reports are inserted into the durable storage outbox atomically with the corresponding
terminal transition. They are acknowledged one at a time only after the host transport reports
successful delivery. If the host transport rejects a report, `AgentRuntimeError::Reporting` retains
the validated `AgentMessage`, and `Agent::replay_outbound_reports` retries the durable row after
reconnect. Delivery is therefore at least once across the narrow transport-send/database-ack
boundary.

Local UI test prints must use `Agent::submit_local_test`, not the durable server-message path. It
accepts a protocol `PrintJob` so it can reuse the same validation, printer resolution, renderer, and
spooler boundaries, but it intentionally creates no server-job row, outbox row, job event, or
outbound protocol report. A host may keep separate desktop-only test history.

## Dependency role

This is the top-level Rust orchestration crate. It may depend on infrastructure crates while those
crates remain independent of it. Printer configuration can come from any backend implementing
`PrinterResolver`; protocol reporting can use any transport implementing `OutboundReporter`.

## Development

```bash
cargo test -p oppa-agent
cargo clippy -p oppa-agent --all-targets -- -D warnings
```

The automated lifecycle tests use SQLite and the virtual spooler. They verify persist-before-report
ordering, exact and idempotency-key duplicate handling, failure persistence, cooperative
cancellation, and restart recovery without physical hardware.
