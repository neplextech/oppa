//! Rendering of validated OpenPrinter documents into backend-ready output.
//!
//! Rendering is deliberately separate from submission. The initial ESC/POS
//! renderer supports reliable ASCII receipt text and raster images. It rejects
//! non-ASCII text explicitly because device code-page behavior cannot provide
//! dependable Nepali Unicode output; a future text-to-raster renderer can fill
//! that boundary without silently corrupting receipts.
//!
//! PNG and JPEG decoders receive strict 2,048-pixel per-axis limits and a
//! 16 MiB allocation budget before decompression begins. Decoder resource-limit
//! failures are reported as [`RendererError::ImageTooLarge`], separately from
//! malformed image data.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::io::Cursor;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use image::{
    DynamicImage, ImageError, ImageReader, Limits, error::LimitErrorKind, imageops::FilterType,
};
use oppa_protocol::{
    BarcodeFormat, ImageMediaType, PrintDocument, PrintSection, ReceiptWidth, TextAlignment,
    Validate,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

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
    /// Raw ESC/POS command bytes.
    EscPos(Vec<u8>),
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
            Self::EscPos(bytes) => bytes.len(),
            Self::Raster(document) => document.pages.iter().map(|page| page.data.len()).sum(),
            Self::Native(document) => document.data.len(),
            Self::Virtual(document) => document.preview_lines.iter().map(String::len).sum(),
        }
    }
}

/// Monochrome raster document for a raster-capable backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RasterDocument {
    /// Ordered pages or receipt segments.
    pub pages: Vec<RasterPage>,
}

/// One tightly packed, one-bit raster page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RasterPage {
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// Row-major bytes, most-significant bit first.
    pub data: Vec<u8>,
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
            RenderTarget::EscPos => RenderedDocument::EscPos(render_esc_pos(document)?),
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
}

/// Result alias for render operations.
pub type RendererResult<T> = Result<T, RendererError>;

fn columns(width: ReceiptWidth) -> usize {
    match width {
        ReceiptWidth::Mm58 => 32,
        ReceiptWidth::Mm80 => 48,
    }
}

fn dots(width: ReceiptWidth) -> u32 {
    match width {
        ReceiptWidth::Mm58 => 384,
        ReceiptWidth::Mm80 => 576,
    }
}

fn render_esc_pos(document: &PrintDocument) -> RendererResult<Vec<u8>> {
    let width = columns(document.width);
    let mut output = vec![0x1b, b'@'];
    for section in &document.sections {
        match section {
            PrintSection::Text { value, align, bold } => {
                require_ascii(value)?;
                output.extend_from_slice(&[0x1b, b'a', alignment_code(*align)]);
                output.extend_from_slice(&[0x1b, b'E', u8::from(bold.unwrap_or(false))]);
                for line in wrap_text(value, width) {
                    output.extend_from_slice(line.as_bytes());
                    output.push(b'\n');
                }
                output.extend_from_slice(&[0x1b, b'E', 0]);
            }
            PrintSection::Row { left, right } => {
                require_ascii(left)?;
                require_ascii(right)?;
                output.extend_from_slice(&[0x1b, b'a', 0]);
                for line in layout_row(left, right, width) {
                    output.extend_from_slice(line.as_bytes());
                    output.push(b'\n');
                }
            }
            PrintSection::Divider => {
                output.extend(std::iter::repeat_n(b'-', width));
                output.push(b'\n');
            }
            PrintSection::Image { media_type, data } => {
                let image = decode_image(*media_type, data)?;
                output.extend(render_image(image, dots(document.width))?);
            }
            PrintSection::Qr { value } => {
                output.extend(render_qr(value));
            }
            PrintSection::Barcode { format, value } => {
                output.extend(render_barcode(*format, value)?);
            }
            PrintSection::Feed { lines } => {
                output.extend_from_slice(&[0x1b, b'd', *lines]);
            }
            PrintSection::Cut => {
                output.extend_from_slice(&[0x1d, b'V', 0]);
            }
        }
    }
    Ok(output)
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

fn render_image(image: DynamicImage, maximum_width: u32) -> RendererResult<Vec<u8>> {
    let image = if image.width() > maximum_width {
        let target_height = image
            .height()
            .saturating_mul(maximum_width)
            .checked_div(image.width())
            .unwrap_or(1)
            .max(1);
        image.resize_exact(maximum_width, target_height, FilterType::Triangle)
    } else {
        image
    }
    .to_luma8();
    let width = image.width();
    let height = image.height();
    let bytes_per_row = width.div_ceil(8);
    let payload_len = usize::try_from(bytes_per_row)
        .ok()
        .and_then(|row| {
            usize::try_from(height)
                .ok()
                .and_then(|height| row.checked_mul(height))
        })
        .ok_or_else(|| RendererError::ImageTooLarge("raster size overflow".to_owned()))?;
    let mut output = Vec::with_capacity(payload_len + 8);
    let width_low = u8::try_from(bytes_per_row & 0xff)
        .map_err(|_| RendererError::ImageTooLarge("raster width overflow".to_owned()))?;
    let width_high = u8::try_from((bytes_per_row >> 8) & 0xff)
        .map_err(|_| RendererError::ImageTooLarge("raster width overflow".to_owned()))?;
    let height_low = u8::try_from(height & 0xff)
        .map_err(|_| RendererError::ImageTooLarge("raster height overflow".to_owned()))?;
    let height_high = u8::try_from((height >> 8) & 0xff)
        .map_err(|_| RendererError::ImageTooLarge("raster height overflow".to_owned()))?;
    output.extend_from_slice(&[
        0x1d,
        b'v',
        b'0',
        0,
        width_low,
        width_high,
        height_low,
        height_high,
    ]);
    for y in 0..height {
        for byte_index in 0..bytes_per_row {
            let mut byte = 0_u8;
            for bit in 0..8 {
                let x = byte_index * 8 + bit;
                if x < width && image.get_pixel(x, y).0[0] < 128 {
                    byte |= 0x80 >> bit;
                }
            }
            output.push(byte);
        }
    }
    Ok(output)
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
        let RenderedDocument::EscPos(bytes) = rendered else {
            panic!("expected ESC/POS");
        };
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
