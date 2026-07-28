# oppa-spooler

`oppa-spooler` submits already rendered documents through bounded concrete
printer transports.

## Implemented spoolers

- Raw TCP with separate connection and write deadlines
- Unix system queues through `lp -d <queue> -o raw`, with no shell evaluation
- In-process virtual printers with bounded history, success/failure/offline
  simulation, fail-next behavior, delay, and cancellation
- `SpoolerRegistry` routing by validated connection family

All backends enforce payload limits, return structured errors, tolerate a
printer disappearing, and support cooperative cancellation where technically
possible. Windows system-queue submission currently returns an explicit
`BackendUnavailable` error; no direct USB spooler is claimed.

A successful `SubmissionReceipt` only means the backend accepted the bytes. It
does not assert physical print completion.

## Primary APIs

- `Spooler`, `SubmissionRequest`, and `SpoolerRegistry`
- `RawTcpSpooler`, `SystemQueueSpooler`, and `VirtualSpooler`
- `VirtualSimulation` and `VirtualSubmission`
- `SpoolerError`

## Development

```bash
cargo test -p oppa-spooler
cargo clippy -p oppa-spooler --all-targets -- -D warnings
```

Tests use loopback TCP and the virtual backend; CI needs no printer.
