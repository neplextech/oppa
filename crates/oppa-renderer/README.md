# oppa-renderer

`oppa-renderer` converts validated OpenPrinter structured documents into printer-ready output
without performing submission.

## Implemented output

- ESC/POS for 58 mm (32-column/384-dot) and 80 mm (48-column/576-dot) receipts
- Text wrapping, alignment, bold, rows, dividers, feed, and cut
- PNG/JPEG raster images with compressed-input, dimension, and decoder-allocation bounds
- Native ESC/POS QR codes and validated Code 128, Code 39, EAN-13, and UPC-A
- Structured virtual output with a readable preview

ESC/POS code pages cannot reliably represent Nepali Unicode across hardware. The renderer therefore
rejects non-ASCII ESC/POS text with `UnicodeRequiresRasterization` instead of emitting corrupted
output. Virtual output preserves Unicode, and the `RasterDocument` boundary is ready for a future
dependable text-to-raster implementation.

## Image safety limits

Image limits are installed on the PNG or JPEG decoder before pixel decompression:

- the base64-decoded compressed source is at most 1,048,576 bytes
- width and height are each at most 2,048 pixels
- the decoder pixel-buffer allocation budget is 16,777,216 bytes

Dimension and allocation failures return `RendererError::ImageTooLarge`; malformed base64 or image
data returns `RendererError::InvalidImage`. The default rendered-document output limit remains
4,194,304 bytes.

## Primary APIs

- `DocumentRenderer`, `RenderTarget`, and `RenderLimits`
- `RenderedDocument`
- `VirtualPrintDocument`, `RasterDocument`, and `NativePrintDocument`

The crate consumes the canonical `oppa-protocol::PrintDocument` and has no printer I/O.
`oppa-spooler` owns transport and timeout behavior.

## Development

```bash
cargo test -p oppa-renderer
cargo clippy -p oppa-renderer --all-targets -- -D warnings
```

Tests require no physical printer.
