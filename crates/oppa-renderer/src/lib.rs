//! Rendering of validated OpenPrinter documents into backend-ready output.
//!
//! Rendering is deliberately separate from submission. The ESC/POS renderer
//! supports reliable ASCII receipt text and raster images. It rejects
//! non-ASCII text explicitly because device code-page behavior cannot provide
//! dependable Unicode output. The page renderer rasterizes text from its
//! embedded glyph set for system-driver printing; unsupported glyphs use a
//! visible fallback instead of becoming printer-control bytes.
//!
//! PNG and JPEG decoders receive strict 2,048-pixel per-axis limits and a
//! 16 MiB allocation budget before decompression begins. Decoder resource-limit
//! failures are reported as [`RendererError::ImageTooLarge`], separately from
//! malformed image data.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::io::Cursor;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use image::{DynamicImage, ImageError, ImageReader, Limits, error::LimitErrorKind};
use oppa_protocol::{
    BarcodeFormat, ImageMediaType, PrintDocument, PrintSection, TextAlignment, Validate,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub use oppa_protocol::ReceiptWidth;

pub mod escpos;
pub mod page;

pub use escpos::{
    EscPosDiagnostics, EscPosInterpretation, EscPosInterpreter, UnsupportedEscPosCommand,
};
pub use page::{
    PageRenderOptions, PrintLayout, ReceiptElement, ReceiptImage, encode_raster_page_png,
    encode_raster_page_preview_png, render_escpos_for_page, render_layout_page,
    render_layout_receipt,
};

/// Default maximum number of bytes emitted for one rendered document.
pub const DEFAULT_MAX_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
/// Maximum base64-decoded, still-compressed source image accepted by the renderer.
pub const MAX_DECODED_IMAGE_BYTES: usize = 1024 * 1024;
/// Maximum pixel-buffer allocation permitted while decoding one image.
pub const MAX_IMAGE_DECODER_ALLOCATION_BYTES: u64 = 16 * 1024 * 1024;
/// Maximum source image dimension accepted before resizing.
pub const MAX_IMAGE_DIMENSION: u32 = 2_048;

/// Output ready for a compatible spooler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderedDocument {
    /// Raw ESC/POS command bytes plus the receipt width used to encode them.
    EscPos(EscPosDocument),
    /// One or more monochrome raster pages.
    Raster(RasterDocument),
    /// Platform-native document payload.
    Native(NativePrintDocument),
    /// Structured virtual-printer output and readable preview.
    Virtual(VirtualPrintDocument),
}

impl RenderedDocument {
    /// Returns a stable output-family name for errors and diagnostics.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::EscPos(_) => "esc-pos",
            Self::Raster(_) => "raster",
            Self::Native(_) => "native",
            Self::Virtual(_) => "virtual",
        }
    }

    /// Returns the approximate in-memory payload size.
    #[must_use]
    pub fn byte_len(&self) -> usize {
        match self {
            Self::EscPos(document) => document.bytes.len(),
            Self::Raster(document) => document.pages.iter().map(|page| page.data.len()).sum(),
            Self::Native(document) => document.data.len(),
            Self::Virtual(document) => document.preview_lines.iter().map(String::len).sum(),
        }
    }
}

/// Printer-language payload with the layout width needed by compatibility
/// interpreters. ESC/POS itself has no standard width declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EscPosDocument {
    /// Raw ESC/POS command bytes.
    pub bytes: Vec<u8>,
    /// Receipt width used when OPPA encoded or configured the byte stream.
    pub receipt_width: ReceiptWidth,
}

/// Monochrome raster document for a raster-capable backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RasterDocument {
    /// Ordered pages or receipt segments.
    pub pages: Vec<RasterPage>,
}

/// Raster page in one of the supported pixel formats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RasterPage {
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// Horizontal resolution represented by this page.
    pub dpi_x: u16,
    /// Vertical resolution represented by this page.
    pub dpi_y: u16,
    /// Pixel encoding used in `data`.
    pub format: PixelFormat,
    /// Row-major bytes, most-significant bit first.
    pub data: Vec<u8>,
}

/// Pixel encodings supported by a raster page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// Packed one-bit black pixels, most-significant bit first, white background.
    Mono1,
    /// One grayscale byte per pixel.
    Gray8,
    /// Three RGB bytes per pixel.
    Rgb24,
    /// Four RGBA bytes per pixel.
    Rgba32,
}

impl RasterPage {
    /// Checks dimensions and verifies the exact packed buffer length.
    pub fn validate(&self) -> RendererResult<()> {
        let bytes_per_row =
            match self.format {
                PixelFormat::Mono1 => self.width.div_ceil(8),
                PixelFormat::Gray8 => self.width,
                PixelFormat::Rgb24 => self.width.checked_mul(3).ok_or_else(|| {
                    RendererError::InvalidRasterPage("row size overflow".to_owned())
                })?,
                PixelFormat::Rgba32 => self.width.checked_mul(4).ok_or_else(|| {
                    RendererError::InvalidRasterPage("row size overflow".to_owned())
                })?,
            };
        let expected = usize::try_from(bytes_per_row)
            .ok()
            .and_then(|row| {
                usize::try_from(self.height)
                    .ok()
                    .and_then(|height| row.checked_mul(height))
            })
            .ok_or_else(|| RendererError::InvalidRasterPage("page size overflow".to_owned()))?;
        if self.width == 0 || self.height == 0 || self.dpi_x == 0 || self.dpi_y == 0 {
            return Err(RendererError::InvalidRasterPage(
                "dimensions and resolution must be non-zero".to_owned(),
            ));
        }
        if self.data.len() != expected {
            return Err(RendererError::InvalidRasterPage(format!(
                "pixel buffer has {} bytes; expected {expected}",
                self.data.len()
            )));
        }
        Ok(())
    }
}

/// Opaque platform-native print content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativePrintDocument {
    /// Registered media type understood by the platform backend.
    pub media_type: String,
    /// Bounded native payload.
    pub data: Vec<u8>,
}

/// Virtual output retained for development and diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VirtualPrintDocument {
    /// Original validated structured document.
    pub document: PrintDocument,
    /// Plain-text approximation suitable for the desktop inspector.
    pub preview_lines: Vec<String>,
}

/// Requested renderer family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderTarget {
    /// ESC/POS bytes for receipt printers.
    EscPos,
    /// Device-independent page raster for an installed printer driver.
    Page(PageRenderOptions),
    /// Structured virtual-printer output.
    Virtual,
}

/// Configurable rendering limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderLimits {
    /// Maximum rendered output bytes.
    pub max_output_bytes: usize,
}

impl Default for RenderLimits {
    fn default() -> Self {
        Self {
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
        }
    }
}

/// Stateless structured-document renderer.
#[derive(Debug, Clone, Copy, Default)]
pub struct DocumentRenderer {
    limits: RenderLimits,
}

impl DocumentRenderer {
    /// Creates a renderer with explicit output bounds.
    pub fn new(limits: RenderLimits) -> RendererResult<Self> {
        if limits.max_output_bytes == 0 {
            return Err(RendererError::InvalidLimits);
        }
        Ok(Self { limits })
    }

    /// Validates and renders a structured document.
    pub fn render(
        &self,
        document: &PrintDocument,
        target: RenderTarget,
    ) -> RendererResult<RenderedDocument> {
        document
            .validate()
            .map_err(|error| RendererError::InvalidDocument(error.to_string()))?;
        let rendered = match target {
            RenderTarget::EscPos => {
                let layout = PrintLayout::from_document(document)?;
                RenderedDocument::EscPos(EscPosDocument {
                    bytes: escpos::render_layout(&layout)?,
                    receipt_width: layout.width,
                })
            }
            RenderTarget::Page(options) => {
                let layout = PrintLayout::from_document(document)?;
                RenderedDocument::Raster(render_layout_page(&layout, options)?)
            }
            RenderTarget::Virtual => RenderedDocument::Virtual(render_virtual(document)),
        };
        let actual = rendered.byte_len();
        if actual > self.limits.max_output_bytes {
            return Err(RendererError::OutputTooLarge {
                actual,
                maximum: self.limits.max_output_bytes,
            });
        }
        Ok(rendered)
    }
}

/// Renderer failures that distinguish invalid content from unsupported output.
#[derive(Debug, Error)]
pub enum RendererError {
    /// Renderer limits were nonsensical.
    #[error("renderer output limit must be greater than zero")]
    InvalidLimits,
    /// A configured physical page size or resolution is unsupported.
    #[error("page dimensions must be 100–500 mm and resolution 72–600 DPI")]
    InvalidPageOptions,
    /// Protocol validation rejected the document.
    #[error("print document is invalid: {0}")]
    InvalidDocument(String),
    /// Reliable device text output requires rasterization.
    #[error(
        "ESC/POS text contains Unicode that is not reliably supported; use a raster text renderer"
    )]
    UnicodeRequiresRasterization,
    /// Encoded image data was malformed.
    #[error("image data is invalid: {0}")]
    InvalidImage(String),
    /// The compressed payload, dimensions, or decoder allocation exceeded a safety limit.
    #[error("image exceeds renderer limits: {0}")]
    ImageTooLarge(String),
    /// Barcode content did not meet its symbology constraints.
    #[error("barcode is invalid for {format:?}: {reason}")]
    InvalidBarcode {
        /// Requested symbology.
        format: BarcodeFormat,
        /// Sanitized validation reason.
        reason: &'static str,
    },
    /// Rendered output exceeded its configured maximum.
    #[error("rendered output is {actual} bytes; maximum is {maximum}")]
    OutputTooLarge {
        /// Actual output size.
        actual: usize,
        /// Configured bound.
        maximum: usize,
    },
    /// Raster page dimensions or pixel data were inconsistent.
    #[error("raster page is invalid: {0}")]
    InvalidRasterPage(String),
    /// A receipt did not fit inside the configured page profile.
    #[error("receipt content exceeds the configured page height")]
    PageOverflow,
    /// A barcode could not be represented as a safe graphical barcode.
    #[error("barcode cannot be rendered: {0}")]
    BarcodeRender(String),
    /// An ESC/POS stream could not be safely interpreted.
    #[error("ESC/POS command stream is unsupported or malformed at byte {offset}: {reason}")]
    InvalidEscPos {
        /// Byte offset where parsing stopped.
        offset: usize,
        /// Bounded parser reason.
        reason: String,
    },
}

/// Result alias for render operations.
pub type RendererResult<T> = Result<T, RendererError>;

fn columns(width: ReceiptWidth) -> usize {
    match width {
        ReceiptWidth::Mm58 => 32,
        ReceiptWidth::Mm80 => 48,
    }
}

fn require_ascii(value: &str) -> RendererResult<()> {
    if value.is_ascii() {
        Ok(())
    } else {
        Err(RendererError::UnicodeRequiresRasterization)
    }
}

fn alignment_code(alignment: Option<TextAlignment>) -> u8 {
    match alignment.unwrap_or(TextAlignment::Left) {
        TextAlignment::Left => 0,
        TextAlignment::Center => 1,
        TextAlignment::Right => 2,
    }
}

fn wrap_text(value: &str, width: usize) -> Vec<String> {
    let mut result = Vec::new();
    for source_line in value.split('\n') {
        if source_line.is_empty() {
            result.push(String::new());
            continue;
        }
        let mut current = String::new();
        for word in source_line.split_whitespace() {
            let candidate_width = UnicodeWidthStr::width(current.as_str())
                + usize::from(!current.is_empty())
                + UnicodeWidthStr::width(word);
            if !current.is_empty() && candidate_width > width {
                result.push(current);
                current = String::new();
            }
            if UnicodeWidthStr::width(word) > width {
                if !current.is_empty() {
                    result.push(std::mem::take(&mut current));
                }
                let mut segment = String::new();
                for grapheme in word.graphemes(true) {
                    if !segment.is_empty()
                        && UnicodeWidthStr::width(segment.as_str())
                            + UnicodeWidthStr::width(grapheme)
                            > width
                    {
                        result.push(std::mem::take(&mut segment));
                    }
                    segment.push_str(grapheme);
                }
                current = segment;
            } else {
                if !current.is_empty() {
                    current.push(' ');
                }
                current.push_str(word);
            }
        }
        if !current.is_empty() {
            result.push(current);
        }
    }
    result
}

fn layout_row(left: &str, right: &str, width: usize) -> Vec<String> {
    if UnicodeWidthStr::width(left) + UnicodeWidthStr::width(right) < width {
        return vec![format!(
            "{left}{}{right}",
            " ".repeat(width - UnicodeWidthStr::width(left) - UnicodeWidthStr::width(right))
        )];
    }
    let left_width = width.saturating_mul(3) / 5;
    let right_width = width.saturating_sub(left_width + 1);
    let left_lines = wrap_text(left, left_width.max(1));
    let right_lines = wrap_text(right, right_width.max(1));
    let count = left_lines.len().max(right_lines.len());
    (0..count)
        .map(|index| {
            let left = left_lines.get(index).map_or("", String::as_str);
            let right = right_lines.get(index).map_or("", String::as_str);
            let spacing = width
                .saturating_sub(UnicodeWidthStr::width(left) + UnicodeWidthStr::width(right))
                .max(1);
            format!("{left}{}{right}", " ".repeat(spacing))
        })
        .collect()
}

fn decode_image(media_type: ImageMediaType, data: &str) -> RendererResult<DynamicImage> {
    let decoded = STANDARD
        .decode(data)
        .map_err(|error| RendererError::InvalidImage(error.to_string()))?;
    if decoded.len() > MAX_DECODED_IMAGE_BYTES {
        return Err(RendererError::ImageTooLarge(format!(
            "decoded image is {} bytes; maximum is {MAX_DECODED_IMAGE_BYTES}",
            decoded.len()
        )));
    }
    let format = match media_type {
        ImageMediaType::Png => image::ImageFormat::Png,
        ImageMediaType::Jpeg => image::ImageFormat::Jpeg,
    };
    let mut reader = ImageReader::with_format(Cursor::new(decoded), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
    limits.max_alloc = Some(MAX_IMAGE_DECODER_ALLOCATION_BYTES);
    reader.limits(limits);
    reader.decode().map_err(map_image_decode_error)
}

fn map_image_decode_error(error: ImageError) -> RendererError {
    let ImageError::Limits(limit) = error else {
        return RendererError::InvalidImage(error.to_string());
    };
    let reason = match limit.kind() {
        LimitErrorKind::DimensionError => {
            format!("width and height must each be at most {MAX_IMAGE_DIMENSION}px")
        }
        LimitErrorKind::InsufficientMemory => {
            format!("decoder allocation exceeds {MAX_IMAGE_DECODER_ALLOCATION_BYTES} bytes")
        }
        LimitErrorKind::Unsupported { .. } => {
            "decoder cannot enforce the required image safety limits".to_owned()
        }
        _ => "decoder resource limit exceeded".to_owned(),
    };
    RendererError::ImageTooLarge(reason)
}

fn render_qr(value: &str) -> Vec<u8> {
    let data_len = value.len() + 3;
    let low = (data_len & 0xff) as u8;
    let high = ((data_len >> 8) & 0xff) as u8;
    let mut output = Vec::with_capacity(value.len() + 40);
    // Model 2, module size 6, medium error correction, store, print.
    output.extend_from_slice(&[0x1d, b'(', b'k', 4, 0, 49, 65, 50, 0]);
    output.extend_from_slice(&[0x1d, b'(', b'k', 3, 0, 49, 67, 6]);
    output.extend_from_slice(&[0x1d, b'(', b'k', 3, 0, 49, 69, 49]);
    output.extend_from_slice(&[0x1d, b'(', b'k', low, high, 49, 80, 48]);
    output.extend_from_slice(value.as_bytes());
    output.extend_from_slice(&[0x1d, b'(', b'k', 3, 0, 49, 81, 48]);
    output
}

fn render_barcode(format: BarcodeFormat, value: &str) -> RendererResult<Vec<u8>> {
    let (command, payload) = match format {
        BarcodeFormat::Code128 => {
            if !value.is_ascii() || value.len() > 253 {
                return Err(RendererError::InvalidBarcode {
                    format,
                    reason: "Code 128 must be at most 253 ASCII bytes",
                });
            }
            (73_u8, format!("{{B{value}").into_bytes())
        }
        BarcodeFormat::Code39 => {
            if !value.bytes().all(|byte| {
                byte.is_ascii_uppercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b' ' | b'-' | b'.' | b'$' | b'/' | b'+' | b'%')
            }) {
                return Err(RendererError::InvalidBarcode {
                    format,
                    reason: "Code 39 contains unsupported characters",
                });
            }
            (69_u8, value.as_bytes().to_vec())
        }
        BarcodeFormat::Ean13 => {
            if !matches!(value.len(), 12 | 13) || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(RendererError::InvalidBarcode {
                    format,
                    reason: "EAN-13 requires 12 or 13 digits",
                });
            }
            (67_u8, value.as_bytes().to_vec())
        }
        BarcodeFormat::Upca => {
            if !matches!(value.len(), 11 | 12) || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(RendererError::InvalidBarcode {
                    format,
                    reason: "UPC-A requires 11 or 12 digits",
                });
            }
            (65_u8, value.as_bytes().to_vec())
        }
    };
    let length = u8::try_from(payload.len()).map_err(|_| RendererError::InvalidBarcode {
        format,
        reason: "barcode payload is too long",
    })?;
    let mut output = vec![0x1d, b'H', 2, 0x1d, b'h', 80, 0x1d, b'k', command, length];
    output.extend(payload);
    Ok(output)
}

fn render_virtual(document: &PrintDocument) -> VirtualPrintDocument {
    let width = columns(document.width);
    let mut preview_lines = Vec::new();
    for section in &document.sections {
        match section {
            PrintSection::Text { value, align, .. } => {
                for line in wrap_text(value, width) {
                    preview_lines.push(align_preview(&line, width, *align));
                }
            }
            PrintSection::Row { left, right } => {
                preview_lines.extend(layout_row(left, right, width));
            }
            PrintSection::Divider => preview_lines.push("-".repeat(width)),
            PrintSection::Image { media_type, .. } => {
                preview_lines.push(format!("[image: {media_type:?}]"));
            }
            PrintSection::Qr { value } => preview_lines.push(format!("[QR: {value}]")),
            PrintSection::Barcode { format, value } => {
                preview_lines.push(format!("[barcode {format:?}: {value}]"));
            }
            PrintSection::Feed { lines } => {
                preview_lines.extend(std::iter::repeat_n(String::new(), usize::from(*lines)));
            }
            PrintSection::Cut => preview_lines.push("[cut]".to_owned()),
        }
    }
    VirtualPrintDocument {
        document: document.clone(),
        preview_lines,
    }
}

fn align_preview(value: &str, width: usize, alignment: Option<TextAlignment>) -> String {
    let padding = width.saturating_sub(UnicodeWidthStr::width(value));
    match alignment.unwrap_or(TextAlignment::Left) {
        TextAlignment::Left => value.to_owned(),
        TextAlignment::Center => format!("{}{value}", " ".repeat(padding / 2)),
        TextAlignment::Right => format!("{}{value}", " ".repeat(padding)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(width: ReceiptWidth, sections: Vec<PrintSection>) -> PrintDocument {
        PrintDocument { width, sections }
    }

    fn encode_test_image(image: &DynamicImage, format: image::ImageFormat) -> String {
        let mut encoded = Cursor::new(Vec::new());
        image
            .write_to(&mut encoded, format)
            .expect("test image must encode");
        STANDARD.encode(encoded.into_inner())
    }

    fn png_header(width: u32, height: u32, bit_depth: u8, color_type: u8) -> String {
        let mut encoded = vec![137, 80, 78, 71, 13, 10, 26, 10];
        let mut header = Vec::with_capacity(13);
        header.extend_from_slice(&width.to_be_bytes());
        header.extend_from_slice(&height.to_be_bytes());
        header.extend_from_slice(&[bit_depth, color_type, 0, 0, 0]);
        push_png_chunk(&mut encoded, *b"IHDR", &header);
        push_png_chunk(&mut encoded, *b"IDAT", &[]);
        push_png_chunk(&mut encoded, *b"IEND", &[]);
        STANDARD.encode(encoded)
    }

    fn push_png_chunk(encoded: &mut Vec<u8>, kind: [u8; 4], data: &[u8]) {
        encoded.extend_from_slice(
            &u32::try_from(data.len())
                .expect("test chunk length must fit")
                .to_be_bytes(),
        );
        let checksum_start = encoded.len();
        encoded.extend_from_slice(&kind);
        encoded.extend_from_slice(data);
        let checksum = png_crc32(&encoded[checksum_start..]);
        encoded.extend_from_slice(&checksum.to_be_bytes());
    }

    fn png_crc32(bytes: &[u8]) -> u32 {
        let mut checksum = u32::MAX;
        for byte in bytes {
            checksum ^= u32::from(*byte);
            for _ in 0..8 {
                let mask = 0_u32.wrapping_sub(checksum & 1);
                checksum = (checksum >> 1) ^ (0xedb8_8320 & mask);
            }
        }
        !checksum
    }

    #[test]
    fn wraps_and_aligns_58mm_receipt_text() {
        let rendered = DocumentRenderer::default()
            .render(
                &document(
                    ReceiptWidth::Mm58,
                    vec![
                        PrintSection::Text {
                            value: "A deliberately long receipt heading that wraps".to_owned(),
                            align: Some(TextAlignment::Center),
                            bold: Some(true),
                        },
                        PrintSection::Row {
                            left: "Coffee".to_owned(),
                            right: "120.00".to_owned(),
                        },
                        PrintSection::Cut,
                    ],
                ),
                RenderTarget::EscPos,
            )
            .expect("render");
        let RenderedDocument::EscPos(document) = rendered else {
            panic!("expected ESC/POS");
        };
        let bytes = document.bytes;
        assert!(bytes.starts_with(&[0x1b, b'@', 0x1b, b'a', 1]));
        assert!(bytes.windows(6).any(|window| window == b"Coffee"));
        assert!(bytes.ends_with(&[0x1d, b'V', 0]));
    }

    #[test]
    fn virtual_renderer_preserves_unicode_without_claiming_escpos_support() {
        let document = document(
            ReceiptWidth::Mm80,
            vec![PrintSection::Text {
                value: "नेपाली रसिद".to_owned(),
                align: Some(TextAlignment::Center),
                bold: None,
            }],
        );
        assert!(matches!(
            DocumentRenderer::default().render(&document, RenderTarget::EscPos),
            Err(RendererError::UnicodeRequiresRasterization)
        ));
        let rendered = DocumentRenderer::default()
            .render(&document, RenderTarget::Virtual)
            .expect("virtual render");
        let RenderedDocument::Virtual(virtual_document) = rendered else {
            panic!("expected virtual");
        };
        assert!(virtual_document.preview_lines[0].contains("नेपाली"));
    }

    #[test]
    fn barcode_constraints_are_explicit() {
        let document = document(
            ReceiptWidth::Mm80,
            vec![PrintSection::Barcode {
                format: BarcodeFormat::Ean13,
                value: "not-digits".to_owned(),
            }],
        );
        assert!(matches!(
            DocumentRenderer::default().render(&document, RenderTarget::EscPos),
            Err(RendererError::InvalidBarcode {
                format: BarcodeFormat::Ean13,
                ..
            })
        ));
    }

    #[test]
    fn output_bound_is_enforced() {
        let renderer = DocumentRenderer::new(RenderLimits {
            max_output_bytes: 4,
        })
        .expect("limits");
        let document = document(ReceiptWidth::Mm58, vec![PrintSection::Divider]);
        assert!(matches!(
            renderer.render(&document, RenderTarget::EscPos),
            Err(RendererError::OutputTooLarge { .. })
        ));
    }

    #[test]
    fn png_and_jpeg_decode_under_installed_limits() {
        for (media_type, format) in [
            (ImageMediaType::Png, image::ImageFormat::Png),
            (ImageMediaType::Jpeg, image::ImageFormat::Jpeg),
        ] {
            let data = encode_test_image(&DynamicImage::new_luma8(2, 2), format);
            let image = decode_image(media_type, &data).expect("bounded image must decode");
            assert_eq!(image.width(), 2);
            assert_eq!(image.height(), 2);
        }
    }

    #[test]
    fn png_dimension_limit_wins_before_compressed_data_is_read() {
        let data = png_header(MAX_IMAGE_DIMENSION + 1, 1, 8, 0);
        assert!(matches!(
            decode_image(ImageMediaType::Png, &data),
            Err(RendererError::ImageTooLarge(reason))
                if reason == "width and height must each be at most 2048px"
        ));
    }

    #[test]
    fn jpeg_dimension_limit_is_mapped_as_image_too_large() {
        let data = encode_test_image(
            &DynamicImage::new_luma8(MAX_IMAGE_DIMENSION + 1, 1),
            image::ImageFormat::Jpeg,
        );
        assert!(matches!(
            decode_image(ImageMediaType::Jpeg, &data),
            Err(RendererError::ImageTooLarge(reason))
                if reason == "width and height must each be at most 2048px"
        ));
    }

    #[test]
    fn decoder_allocation_limit_wins_before_png_decompression() {
        let data = png_header(MAX_IMAGE_DIMENSION, MAX_IMAGE_DIMENSION, 16, 6);
        assert!(matches!(
            decode_image(ImageMediaType::Png, &data),
            Err(RendererError::ImageTooLarge(reason))
                if reason == "decoder allocation exceeds 16777216 bytes"
        ));
    }
}
