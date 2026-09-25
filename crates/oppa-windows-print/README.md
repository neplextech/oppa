# `oppa-windows-print`

`oppa-windows-print` submits monochrome page rasters to installed Windows
printer queues through GDI. The selected queue's driver receives device
independent drawing operations and converts them to its native printer format.

The crate owns the narrow Windows GDI boundary and isolates its unsafe FFI in a
single module. Its safe print-sequence controller is platform independent and
tested with fakes, so queue lifecycle and cleanup behavior do not need physical
hardware.

It does not render OPPA documents, emit printer-native languages, discover
queues, open dialogs, or claim that a completed spooler submission produced
physical output. `oppa-renderer` produces the page raster and `oppa-spooler`
selects this backend for configured Windows system queues. On Unix, the
spooler submits page files with `lp` in driver/filter mode so CUPS and the
installed queue configuration handle conversion; this Windows-only crate is
not a dependency on those targets.

On Windows, the public entry point is `print_document(queue_name, document)`.
The lower-level `DriverPrintApi` trait supports controlled integration tests.
Run `cargo test -p oppa-windows-print` and the workspace checks to validate
this crate.
