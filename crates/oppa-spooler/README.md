# oppa-spooler

`oppa-spooler` submits rendered output through bounded printer transports.
Printer connection and document language are separate: a system queue may use
its installed driver or an explicitly selected raw printer language.

## Implemented spoolers

- PDF documents through CUPS native submission on Unix, or rasterized GDI
  pages on Windows, as described below
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

## PDF routes

`RenderedDocument::Pdf` is submitted through a route chosen by the host
platform:

- **Unix (macOS and Linux):** the validated PDF is sent to `lp` with IPP
  options for copies, media, orientation, sides, page ranges, and scaling.
  CUPS applies the driver's own PDF filters. Scaling defaults to actual size.
  The temporary file is owner-only and removed after `lp` completes.
- **Windows:** pages are rasterized and sent through the existing GDI path.
  Duplex is rejected with `UnsupportedOption` because the GDI path uses the
  driver's default device mode.

PDF jobs require driver submission mode. Virtual office printers rasterize at
100 DPI for inspection, bounded by a 64 MiB history budget shared with other
virtual output.

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
