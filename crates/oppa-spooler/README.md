# oppa-spooler

`oppa-spooler` submits rendered output through bounded printer transports.
Printer connection and document language are separate: a system queue may use
its installed driver or an explicitly selected raw printer language.

## Implemented spoolers

- Raw TCP for explicitly configured ESC/POS printers, with separate connection
  and write deadlines
- Unix raw system queues through `lp -d <queue> -o raw`; driver-mode queues
  receive PNG pages through `lp` without raw mode so CUPS can apply filters
- Windows raw queues use `WritePrinter` only for explicitly configured
  ESC/POS; driver-mode queues use silent GDI printing through the installed
  driver in `oppa-windows-print`
- Driver queues can interpret the supported OPPA ESC/POS subset into a page
  before driver submission, for compatibility with ESC/POS-producing clients
- Virtual thermal printers interpret the actual emitted ESC/POS bytes;
  virtual office printers capture the same page raster passed toward a driver
- Per-printer simulation policies for success, failure, offline state, and
  delay, independent of the virtual printer profile
- `SpoolerRegistry` routing by validated connection family

All backends enforce payload limits, return structured errors, tolerate a
printer disappearing, and support cooperative cancellation where technically
possible. Platforms that are neither Unix nor Windows return an explicit
`BackendUnavailable` error for system-queue submission; no direct USB spooler
is claimed. Generic discovered system queues default to driver mode. Select
raw ESC/POS only for a printer known to accept that command language. A queue
accepting a RAW job does not prove that its device printed it.

A successful `SubmissionReceipt` only means the backend accepted the bytes. It
does not assert physical print completion.

## Primary APIs

- `Spooler`, `SubmissionRequest`, and `SpoolerRegistry`
- `RawTcpSpooler`, `SystemQueueSpooler`, and `VirtualSpooler`
- `VirtualSimulation`, `VirtualSubmission`, and `VirtualPreview`
- `SpoolerError`

## Development

```bash
cargo test -p oppa-spooler
cargo clippy -p oppa-spooler --all-targets -- -D warnings
```

Tests use loopback TCP, captured CUPS arguments, and virtual thermal/office
profiles. CI needs no printer.
