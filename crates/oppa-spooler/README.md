# oppa-spooler

`oppa-spooler` submits already rendered documents through bounded concrete
printer transports.

## Implemented spoolers

- Raw TCP with separate connection and write deadlines
- Unix system queues through `lp -d <queue> -o raw`, with no shell evaluation
- Windows system queues through the Win32 spooler (`OpenPrinter` /
  `StartDocPrinter` with the `RAW` datatype / `WritePrinter`) on a blocking
  task, with an explicit deadline and cooperative cancellation
- In-process virtual printers with bounded history, success/failure/offline
  simulation, fail-next behavior, delay, and cancellation
- `SpoolerRegistry` routing by validated connection family

All backends enforce payload limits, return structured errors, tolerate a
printer disappearing, and support cooperative cancellation where technically
possible. Platforms that are neither Unix nor Windows return an explicit
`BackendUnavailable` error for system-queue submission; no direct USB spooler
is claimed (USB receipt printers install as a system queue and use that path).

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
