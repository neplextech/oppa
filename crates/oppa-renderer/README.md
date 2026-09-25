# oppa-renderer

`oppa-renderer` converts validated OpenPrinter structured documents and
supported ESC/POS streams into output for receipt printers, system drivers,
and virtual printer profiles. It never submits jobs.

## Implemented output

- ESC/POS for 58 mm (32-column/384-dot) and 80 mm (48-column/576-dot) receipts
- Text wrapping, alignment, bold, rows, dividers, feed, and cut
- PNG/JPEG raster images with compressed-input, dimension, and decoder-allocation bounds
- Native ESC/POS QR codes and validated Code 128, Code 39, EAN-13, and UPC-A
- A monochrome page renderer for office-driver printing, with receipt content
  kept at its physical 58 mm or 80 mm width and placed at the top-center of a
  configurable page
- An ESC/POS interpreter for the subset emitted by OPPA: reset, alignment,
  bold, text, line feed, raster images, QR, barcode, and cut
- ESC/POS-to-page compatibility rendering; cut is recorded as metadata and
  does not create page output
- Structured virtual output with a readable preview

ESC/POS code pages cannot reliably represent Nepali Unicode across hardware. The renderer therefore
rejects non-ASCII ESC/POS text with `UnicodeRequiresRasterization` instead of emitting corrupted
output. Driver-page rendering rasterizes text with its embedded 8-by-8 glyph set; characters outside
that set use a question-mark fallback. Virtual office output uses the same page raster passed to the
driver boundary.

The interpreter deliberately supports only the command subset emitted by the current OPPA renderer.
Unknown commands stop interpretation so trailing binary data is never mistaken for printable text.

## Image safety limits

Image limits are installed on the PNG or JPEG decoder before pixel decompression:

- the base64-decoded compressed source is at most 1,048,576 bytes
- width and height are each at most 2,048 pixels
- the decoder pixel-buffer allocation budget is 16,777,216 bytes

Dimension and allocation failures return `RendererError::ImageTooLarge`; malformed base64 or image
data returns `RendererError::InvalidImage`. The default rendered-document output limit remains
4,194,304 bytes.

## Primary APIs

- `DocumentRenderer`, `RenderTarget`, `PageRenderOptions`, and `RenderLimits`
- `RenderedDocument`
- `RasterPage`, `RasterDocument`, and `PixelFormat`
- `EscPosInterpreter`, `EscPosDiagnostics`, and `PrintLayout`
- `VirtualPrintDocument` and `NativePrintDocument`

The crate consumes the canonical `oppa-protocol::PrintDocument` and has no printer I/O.
`oppa-spooler` owns transport and timeout behavior.

## Development

```bash
cargo test -p oppa-renderer
cargo clippy -p oppa-renderer --all-targets -- -D warnings
```

Tests require no physical printer.
