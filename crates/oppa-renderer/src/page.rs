//! Device-independent receipt layout and monochrome page rasterization.

use std::io::Cursor;

use barcoders::sym::{
    code39::Code39,
    code128::Code128,
    ean13::{EAN13, UPCA},
};
use font8x8::{BASIC_FONTS, UnicodeFonts};
use image::{GrayImage, ImageEncoder, Luma, imageops::FilterType};
use oppa_protocol::{
    BarcodeFormat, PrintDocument, PrintSection, ReceiptWidth, TextAlignment, Validate,
};
use qrcode::{Color, QrCode};

use crate::{
    EscPosDiagnostics, EscPosInterpreter, PixelFormat, RasterDocument, RasterPage, RendererError,
    RendererResult,
};

/// Maximum image area allocated for one rendered page.
const MAX_PAGE_PIXELS: u64 = 42_000_000;
/// Maximum raster image height retained in a receipt layout.
const MAX_RECEIPT_IMAGE_HEIGHT: u32 = 8_192;
/// Bitmap glyph scale at 300 DPI; this yields readable receipt-sized text.
const GLYPH_SCALE_AT_300_DPI: u32 = 3;
/// Minimum page margin above the start of the receipt content.
const TOP_MARGIN_MM: u16 = 12;
/// Human-readable barcode text is rendered at this scale.
const HRI_GLYPH_SCALE: u32 = 2;
const MAX_PREVIEW_WIDTH: u32 = 800;
const MAX_PREVIEW_HEIGHT: u32 = 1_200;

/// Page size and resolution for driver-based printing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageRenderOptions {
    /// Page width in millimetres.
    pub width_mm: u16,
    /// Page height in millimetres.
    pub height_mm: u16,
    /// Raster resolution in dots per inch.
    pub dpi: u16,
}

impl PageRenderOptions {
    /// A4 portrait at 300 DPI.
    pub const A4_PORTRAIT: Self = Self {
        width_mm: 210,
        height_mm: 297,
        dpi: 300,
    };

    /// Validates physical page dimensions and rendering resolution.
    pub fn validate(self) -> RendererResult<()> {
        if !(100..=500).contains(&self.width_mm)
            || !(100..=500).contains(&self.height_mm)
            || !(72..=600).contains(&self.dpi)
        {
            return Err(RendererError::InvalidPageOptions);
        }
        Ok(())
    }
}

/// Compact rendering representation shared by structured and interpreted jobs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintLayout {
    /// Intended physical width of the receipt content.
    pub width: ReceiptWidth,
    /// Ordered receipt drawing and device operations.
    pub elements: Vec<ReceiptElement>,
}

impl PrintLayout {
    /// Converts a validated structured document into the common layout.
    pub fn from_document(document: &PrintDocument) -> RendererResult<Self> {
        document
            .validate()
            .map_err(|error| RendererError::InvalidDocument(error.to_string()))?;
        let mut elements = Vec::with_capacity(document.sections.len());
        for section in &document.sections {
            elements.push(match section {
                PrintSection::Text { value, align, bold } => ReceiptElement::Text {
                    value: value.clone(),
                    alignment: align.unwrap_or(TextAlignment::Left),
                    bold: bold.unwrap_or(false),
                },
                PrintSection::Row { left, right } => ReceiptElement::Row {
                    left: left.clone(),
                    right: right.clone(),
                },
                PrintSection::Divider => ReceiptElement::Divider,
                PrintSection::Image { media_type, data } => {
                    let decoded = super::decode_image(*media_type, data)?;
                    let maximum_width = receipt_pixel_width(document.width, 203);
                    let decoded = if decoded.width() > maximum_width {
                        let height = decoded
                            .height()
                            .saturating_mul(maximum_width)
                            .checked_div(decoded.width())
                            .unwrap_or(1)
                            .max(1);
                        decoded.resize_exact(maximum_width, height, FilterType::Triangle)
                    } else {
                        decoded
                    };
                    ReceiptElement::Image(ReceiptImage::from_gray(decoded.to_luma8()))
                }
                PrintSection::Qr { value } => ReceiptElement::Qr(value.clone()),
                PrintSection::Barcode { format, value } => ReceiptElement::Barcode {
                    format: *format,
                    value: value.clone(),
                    height_dots: 80,
                    human_readable: true,
                },
                PrintSection::Feed { lines } => ReceiptElement::Feed { lines: *lines },
                PrintSection::Cut => ReceiptElement::Cut,
            });
        }
        Ok(Self {
            width: document.width,
            elements,
        })
    }
}

/// One semantic receipt drawing operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReceiptElement {
    /// Positioned, wrapped text.
    Text {
        /// Text value.
        value: String,
        /// Horizontal alignment.
        alignment: TextAlignment,
        /// Whether to use a heavier glyph stroke.
        bold: bool,
    },
    /// Two-column row.
    Row {
        /// Leading column.
        left: String,
        /// Trailing column.
        right: String,
    },
    /// Horizontal rule across the receipt width.
    Divider,
    /// Monochrome source image.
    Image(ReceiptImage),
    /// QR code content.
    Qr(String),
    /// Linear barcode content.
    Barcode {
        /// Barcode symbology.
        format: BarcodeFormat,
        /// Printable content.
        value: String,
        /// Requested barcode bar height in printer dots.
        height_dots: u8,
        /// Whether the ESC/POS stream requested human-readable text.
        human_readable: bool,
    },
    /// Vertical feed represented as whitespace on a page.
    Feed {
        /// Number of receipt line advances.
        lines: u8,
    },
    /// Device-specific request to cut paper. Driver pages render no ink for it.
    Cut,
}

/// One monochrome image in the shared receipt layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptImage {
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Resolution represented by the packed source pixels.
    pub dpi: u16,
    /// Number of packed bytes in each source row.
    pub stride: u32,
    /// Packed black bits, most-significant bit first.
    pub data: Vec<u8>,
}

impl ReceiptImage {
    /// Creates a packed image from grayscale pixels using a fixed threshold.
    pub fn from_gray(image: GrayImage) -> Self {
        let width = image.width();
        let height = image.height();
        let stride = width.div_ceil(8);
        let mut data = vec![0_u8; stride as usize * height as usize];
        for y in 0..height {
            for x in 0..width {
                if image.get_pixel(x, y).0[0] < 160 {
                    set_packed_bit(&mut data, stride, x, y);
                }
            }
        }
        Self {
            width,
            height,
            dpi: 203,
            stride,
            data,
        }
    }

    /// Creates an image from an already packed ESC/POS raster block.
    pub fn from_mono(width: u32, height: u32, data: Vec<u8>) -> RendererResult<Self> {
        let stride = width.div_ceil(8);
        let expected = usize::try_from(stride)
            .ok()
            .and_then(|row| {
                usize::try_from(height)
                    .ok()
                    .and_then(|rows| row.checked_mul(rows))
            })
            .ok_or_else(|| RendererError::InvalidRasterPage("image size overflow".to_owned()))?;
        if width == 0 || height == 0 || data.len() != expected {
            return Err(RendererError::InvalidRasterPage(
                "ESC/POS image dimensions do not match its payload".to_owned(),
            ));
        }
        Ok(Self {
            width,
            height,
            dpi: 203,
            stride,
            data,
        })
    }
}

/// Interprets ESC/POS bytes and renders the result on a system-driver page.
pub fn render_escpos_for_page(
    bytes: &[u8],
    receipt_width: ReceiptWidth,
    options: PageRenderOptions,
) -> RendererResult<(RasterDocument, EscPosDiagnostics)> {
    let interpretation = EscPosInterpreter::new(receipt_width).interpret(bytes)?;
    if let Some(unsupported) = interpretation.diagnostics.unsupported_commands.first() {
        return Err(RendererError::InvalidEscPos {
            offset: unsupported.offset,
            reason: unsupported.command.clone(),
        });
    }
    let page = render_layout_page(&interpretation.layout, options)?;
    Ok((page, interpretation.diagnostics))
}

/// Renders the shared layout onto a physical page, centering its receipt width.
pub fn render_layout_page(
    layout: &PrintLayout,
    options: PageRenderOptions,
) -> RendererResult<RasterDocument> {
    options.validate()?;
    let width = mm_to_px(options.width_mm, options.dpi)?;
    let height = mm_to_px(options.height_mm, options.dpi)?;
    validate_canvas_area(width, height)?;
    let receipt_width = receipt_pixel_width(layout.width, options.dpi).min(width);
    let x = width.saturating_sub(receipt_width) / 2;
    let y = mm_to_px(TOP_MARGIN_MM, options.dpi)?;
    let mut canvas = MonoCanvas::new(width, height, options.dpi)?;
    render_elements(layout, &mut canvas, x, y, receipt_width)?;
    Ok(RasterDocument {
        pages: vec![canvas.into_page(options.dpi)],
    })
}

/// Renders the shared layout as a tall receipt-like preview.
pub fn render_layout_receipt(layout: &PrintLayout, dpi: u16) -> RendererResult<RasterDocument> {
    if !(72..=600).contains(&dpi) {
        return Err(RendererError::InvalidPageOptions);
    }
    let width = receipt_pixel_width(layout.width, dpi);
    let estimated_height = estimate_layout_height(layout, dpi)?;
    validate_canvas_area(width, estimated_height)?;
    let mut canvas = MonoCanvas::new(width, estimated_height, dpi)?;
    let final_y = render_elements(layout, &mut canvas, 0, 0, width)?;
    let page = canvas.into_page(dpi);
    let content_height = final_y.max(1).min(page.height);
    let page = crop_page_height(page, content_height)?;
    Ok(RasterDocument { pages: vec![page] })
}

/// Encodes a checked monochrome raster page as a PNG for local previews.
pub fn encode_raster_page_png(page: &RasterPage) -> RendererResult<Vec<u8>> {
    page.validate()?;
    encode_gray_png(page_to_gray(page)?)
}

/// Encodes a bounded-size PNG preview while preserving the page aspect ratio.
pub fn encode_raster_page_preview_png(page: &RasterPage) -> RendererResult<Vec<u8>> {
    page.validate()?;
    let gray = page_to_gray(page)?;
    let scale = (f64::from(MAX_PREVIEW_WIDTH) / f64::from(gray.width()))
        .min(f64::from(MAX_PREVIEW_HEIGHT) / f64::from(gray.height()))
        .min(1.0);
    let preview = if scale < 1.0 {
        image::imageops::resize(
            &gray,
            (f64::from(gray.width()) * scale).round().max(1.0) as u32,
            (f64::from(gray.height()) * scale).round().max(1.0) as u32,
            FilterType::Triangle,
        )
    } else {
        gray
    };
    encode_gray_png(preview)
}

fn encode_gray_png(gray: GrayImage) -> RendererResult<Vec<u8>> {
    let mut encoded = Cursor::new(Vec::new());
    image::codecs::png::PngEncoder::new(&mut encoded)
        .write_image(
            gray.as_raw(),
            gray.width(),
            gray.height(),
            image::ExtendedColorType::L8,
        )
        .map_err(|error| RendererError::InvalidImage(error.to_string()))?;
    Ok(encoded.into_inner())
}

fn render_elements(
    layout: &PrintLayout,
    canvas: &mut MonoCanvas,
    left: u32,
    mut y: u32,
    receipt_width: u32,
) -> RendererResult<u32> {
    let scale = glyph_scale(canvas.dpi);
    let glyph_width = 8 * scale;
    let line_height = 9 * scale;
    let max_chars = usize::try_from(receipt_width / glyph_width.max(1))
        .unwrap_or(1)
        .max(1);

    for element in &layout.elements {
        match element {
            ReceiptElement::Text {
                value,
                alignment,
                bold,
            } => {
                for line in super::wrap_text(value, max_chars) {
                    draw_text(
                        canvas,
                        &line,
                        TextPlacement {
                            alignment: *alignment,
                            bold: *bold,
                            left,
                            y,
                            max_width: receipt_width,
                            scale,
                        },
                    );
                    y = y.saturating_add(line_height);
                }
            }
            ReceiptElement::Row { left: first, right } => {
                let left_chars = max_chars.saturating_mul(3) / 5;
                let right_chars = max_chars.saturating_sub(left_chars + 1).max(1);
                let left_lines = super::wrap_text(first, left_chars.max(1));
                let right_lines = super::wrap_text(right, right_chars);
                let lines = left_lines.len().max(right_lines.len());
                for index in 0..lines {
                    if let Some(line) = left_lines.get(index) {
                        draw_text(
                            canvas,
                            line,
                            TextPlacement {
                                alignment: TextAlignment::Left,
                                bold: false,
                                left,
                                y,
                                max_width: receipt_width,
                                scale,
                            },
                        );
                    }
                    if let Some(line) = right_lines.get(index) {
                        draw_text(
                            canvas,
                            line,
                            TextPlacement {
                                alignment: TextAlignment::Right,
                                bold: false,
                                left,
                                y,
                                max_width: receipt_width,
                                scale,
                            },
                        );
                    }
                    y = y.saturating_add(line_height);
                }
            }
            ReceiptElement::Divider => {
                let thickness = u32::from((canvas.dpi / 150).max(1));
                canvas.fill_rect(
                    left,
                    y.saturating_add(line_height / 2),
                    receipt_width,
                    thickness,
                );
                y = y.saturating_add(line_height);
            }
            ReceiptElement::Image(image) => {
                let source_dpi = u32::from(image.dpi.max(1));
                let target_dpi = u32::from(canvas.dpi);
                let image_width = image.width.saturating_mul(target_dpi) / source_dpi;
                let image_height = image.height.saturating_mul(target_dpi) / source_dpi;
                let (target_width, target_height) = fit_dimensions(
                    image_width.max(1),
                    image_height.max(1),
                    receipt_width,
                    MAX_RECEIPT_IMAGE_HEIGHT.min(canvas.height.saturating_sub(y)),
                );
                canvas.blit_receipt_image(image, left, y, target_width, target_height);
                y = y
                    .saturating_add(target_height)
                    .saturating_add(scale.saturating_mul(2));
            }
            ReceiptElement::Qr(value) => {
                let code = QrCode::new(value.as_bytes())
                    .map_err(|error| RendererError::InvalidImage(error.to_string()))?;
                let modules = u32::try_from(code.width()).unwrap_or(u32::MAX);
                let total_modules = modules.saturating_add(8);
                let nominal_module = (u32::from(canvas.dpi) * 6 / 203).max(1);
                let module = nominal_module
                    .min(receipt_width / total_modules.max(1))
                    .max(1);
                let code_width = total_modules.saturating_mul(module);
                let code_x = left.saturating_add(receipt_width.saturating_sub(code_width) / 2);
                let quiet = 4 * module;
                for module_y in 0..modules {
                    for module_x in 0..modules {
                        if code[(module_x as usize, module_y as usize)] == Color::Dark {
                            canvas.fill_rect(
                                code_x + quiet + module_x * module,
                                y + quiet + module_y * module,
                                module,
                                module,
                            );
                        }
                    }
                }
                y = y.saturating_add(code_width).saturating_add(scale * 2);
            }
            ReceiptElement::Barcode {
                format,
                value,
                height_dots,
                human_readable,
            } => {
                let encoded = encode_barcode(*format, value)?;
                let quiet = 10_u32;
                let available = receipt_width.saturating_sub(quiet * 2).max(1);
                if u32::try_from(encoded.len()).unwrap_or(u32::MAX) > available {
                    return Err(RendererError::BarcodeRender(
                        "encoded barcode is wider than the receipt".to_owned(),
                    ));
                }
                let module_scale = (available / u32::try_from(encoded.len()).unwrap_or(1))
                    .clamp(1, (u32::from(canvas.dpi) / 100).max(1));
                let barcode_width = u32::try_from(encoded.len()).unwrap_or(0) * module_scale;
                let barcode_x =
                    left.saturating_add(receipt_width.saturating_sub(barcode_width) / 2);
                let barcode_height = (u32::from(canvas.dpi) * u32::from(*height_dots) / 203).max(1);
                for (index, bar) in encoded.iter().enumerate() {
                    if *bar == 1 {
                        canvas.fill_rect(
                            barcode_x + u32::try_from(index).unwrap_or(0) * module_scale,
                            y,
                            module_scale,
                            barcode_height,
                        );
                    }
                }
                y = y.saturating_add(barcode_height).saturating_add(2 * scale);
                if *human_readable {
                    draw_text(
                        canvas,
                        value,
                        TextPlacement {
                            alignment: TextAlignment::Center,
                            bold: false,
                            left,
                            y,
                            max_width: receipt_width,
                            scale: HRI_GLYPH_SCALE.min(scale),
                        },
                    );
                    y = y.saturating_add(line_height);
                }
            }
            ReceiptElement::Feed { lines } => {
                y = y.saturating_add(u32::from(*lines) * line_height);
            }
            ReceiptElement::Cut => {
                // Cutting is a device action and intentionally draws no pixels.
            }
        }
        if y > canvas.height {
            return Err(RendererError::PageOverflow);
        }
    }
    Ok(y)
}

fn estimate_layout_height(layout: &PrintLayout, dpi: u16) -> RendererResult<u32> {
    let scale = glyph_scale(dpi);
    let line_height = 9 * scale;
    let width = receipt_pixel_width(layout.width, dpi);
    let max_chars = usize::try_from(width / (8 * scale).max(1))
        .unwrap_or(1)
        .max(1);
    let mut height = 4 * scale;
    for element in &layout.elements {
        height = height.saturating_add(match element {
            ReceiptElement::Text { value, .. } => {
                let lines = super::wrap_text(value, max_chars).len().max(1);
                u32::try_from(lines).unwrap_or(u32::MAX) * line_height
            }
            ReceiptElement::Row { left, right } => {
                let lines = super::wrap_text(left, (max_chars * 3 / 5).max(1))
                    .len()
                    .max(super::wrap_text(right, (max_chars * 2 / 5).max(1)).len())
                    .max(1);
                u32::try_from(lines).unwrap_or(u32::MAX) * line_height
            }
            ReceiptElement::Divider => line_height,
            ReceiptElement::Image(image) => {
                image
                    .height
                    .saturating_mul(u32::from(dpi))
                    .checked_div(u32::from(image.dpi.max(1)))
                    .unwrap_or(u32::MAX)
                    .min(MAX_RECEIPT_IMAGE_HEIGHT)
                    + scale * 2
            }
            ReceiptElement::Qr(value) => {
                let code = QrCode::new(value.as_bytes())
                    .map_err(|error| RendererError::InvalidImage(error.to_string()))?;
                u32::try_from(code.width())
                    .unwrap_or(u32::MAX)
                    .saturating_add(8)
                    .saturating_mul((u32::from(dpi) * 6 / 203).max(1))
                    .saturating_add(scale * 2)
            }
            ReceiptElement::Barcode {
                format,
                value,
                height_dots,
                human_readable,
            } => {
                let _ = encode_barcode(*format, value)?;
                u32::from(dpi) * u32::from(*height_dots) / 203
                    + if *human_readable { line_height } else { 0 }
                    + scale * 2
            }
            ReceiptElement::Feed { lines } => u32::from(*lines) * line_height,
            ReceiptElement::Cut => 0,
        });
    }
    Ok(height.max(1))
}

fn encode_barcode(format: BarcodeFormat, value: &str) -> RendererResult<Vec<u8>> {
    match format {
        BarcodeFormat::Code128 => {
            let input = format!("\u{0181}{value}");
            Code128::new(input)
                .map(|barcode| barcode.encode())
                .map_err(|error| RendererError::BarcodeRender(error.to_string()))
        }
        BarcodeFormat::Code39 => Code39::new(value)
            .map(|barcode| barcode.encode())
            .map_err(|error| RendererError::BarcodeRender(error.to_string())),
        BarcodeFormat::Ean13 => EAN13::new(value)
            .map(|barcode| barcode.encode())
            .map_err(|error| RendererError::BarcodeRender(error.to_string())),
        BarcodeFormat::Upca => UPCA::new(value)
            .map(|barcode| barcode.encode())
            .map_err(|error| RendererError::BarcodeRender(error.to_string())),
    }
}

#[derive(Clone, Copy)]
struct TextPlacement {
    alignment: TextAlignment,
    bold: bool,
    left: u32,
    y: u32,
    max_width: u32,
    scale: u32,
}

fn draw_text(canvas: &mut MonoCanvas, value: &str, placement: TextPlacement) {
    let TextPlacement {
        alignment,
        bold,
        left,
        y,
        max_width,
        scale,
    } = placement;
    let glyph_width = 8 * scale;
    let text_width = u32::try_from(value.chars().count())
        .unwrap_or(u32::MAX)
        .saturating_mul(glyph_width);
    let offset = match alignment {
        TextAlignment::Left => 0,
        TextAlignment::Center => max_width.saturating_sub(text_width) / 2,
        TextAlignment::Right => max_width.saturating_sub(text_width),
    };
    let mut x = left.saturating_add(offset);
    for character in value.chars() {
        let glyph = BASIC_FONTS.get(character).or_else(|| BASIC_FONTS.get('?'));
        if let Some(glyph) = glyph {
            for (row, bits) in glyph.iter().enumerate() {
                for column in 0..8 {
                    if bits & (1 << column) != 0 {
                        let px = x + column * scale;
                        let py = y + u32::try_from(row).unwrap_or(0) * scale;
                        canvas.fill_rect(px, py, scale, scale);
                        if bold {
                            canvas.fill_rect(px.saturating_add(scale), py, 1, scale);
                        }
                    }
                }
            }
        }
        x = x.saturating_add(glyph_width);
    }
}

fn fit_dimensions(
    source_width: u32,
    source_height: u32,
    max_width: u32,
    max_height: u32,
) -> (u32, u32) {
    if source_width == 0 || source_height == 0 || max_width == 0 || max_height == 0 {
        return (0, 0);
    }
    let width_ratio = max_width as f64 / f64::from(source_width);
    let height_ratio = max_height as f64 / f64::from(source_height);
    let ratio = width_ratio.min(height_ratio).min(1.0);
    (
        (f64::from(source_width) * ratio).round().max(1.0) as u32,
        (f64::from(source_height) * ratio).round().max(1.0) as u32,
    )
}

fn receipt_pixel_width(width: ReceiptWidth, dpi: u16) -> u32 {
    let mm = width.millimetres();
    mm_to_px(mm, dpi).unwrap_or(1)
}

fn mm_to_px(mm: u16, dpi: u16) -> RendererResult<u32> {
    u32::from(mm)
        .checked_mul(u32::from(dpi))
        .and_then(|dots| dots.checked_mul(10))
        .and_then(|dots| dots.checked_add(127))
        .map(|dots| dots / 254)
        .ok_or_else(|| RendererError::InvalidRasterPage("dimension overflow".to_owned()))
}

fn glyph_scale(dpi: u16) -> u32 {
    (u32::from(dpi) * GLYPH_SCALE_AT_300_DPI / 300).max(1)
}

fn validate_canvas_area(width: u32, height: u32) -> RendererResult<()> {
    if u64::from(width).saturating_mul(u64::from(height)) > MAX_PAGE_PIXELS {
        return Err(RendererError::OutputTooLarge {
            actual: usize::try_from(u64::from(width) * u64::from(height) / 8).unwrap_or(usize::MAX),
            maximum: usize::try_from(MAX_PAGE_PIXELS / 8).unwrap_or(usize::MAX),
        });
    }
    Ok(())
}

fn crop_page_height(mut page: RasterPage, height: u32) -> RendererResult<RasterPage> {
    if height >= page.height {
        return Ok(page);
    }
    let old_stride = page.width.div_ceil(8);
    let new_len = usize::try_from(old_stride)
        .ok()
        .and_then(|stride| {
            usize::try_from(height)
                .ok()
                .and_then(|rows| stride.checked_mul(rows))
        })
        .ok_or_else(|| RendererError::InvalidRasterPage("crop size overflow".to_owned()))?;
    page.data.truncate(new_len);
    page.height = height;
    page.validate()?;
    Ok(page)
}

fn page_to_gray(page: &RasterPage) -> RendererResult<GrayImage> {
    let mut image = GrayImage::from_pixel(page.width, page.height, Luma([255]));
    match page.format {
        PixelFormat::Mono1 => {
            for y in 0..page.height {
                for x in 0..page.width {
                    if mono_page_pixel(page, x, y) {
                        image.put_pixel(x, y, Luma([0]));
                    }
                }
            }
        }
        PixelFormat::Gray8 => {
            for y in 0..page.height {
                for x in 0..page.width {
                    let index = usize::try_from(y * page.width + x).unwrap_or(usize::MAX);
                    image.put_pixel(x, y, Luma([page.data[index]]));
                }
            }
        }
        PixelFormat::Rgb24 | PixelFormat::Rgba32 => {
            let channels = if page.format == PixelFormat::Rgb24 {
                3
            } else {
                4
            };
            for y in 0..page.height {
                for x in 0..page.width {
                    let index =
                        usize::try_from((y * page.width + x) * channels).unwrap_or(usize::MAX);
                    let red = u32::from(page.data[index]);
                    let green = u32::from(page.data[index + 1]);
                    let blue = u32::from(page.data[index + 2]);
                    let luminance = ((red * 299 + green * 587 + blue * 114) / 1_000) as u8;
                    image.put_pixel(x, y, Luma([luminance]));
                }
            }
        }
    }
    Ok(image)
}

fn mono_page_pixel(page: &RasterPage, x: u32, y: u32) -> bool {
    if page.format != PixelFormat::Mono1 || x >= page.width || y >= page.height {
        return false;
    }
    let stride = page.width.div_ceil(8);
    let index = usize::try_from(y * stride + x / 8).unwrap_or(usize::MAX);
    page.data
        .get(index)
        .is_some_and(|byte| byte & (0x80 >> (x % 8)) != 0)
}

fn set_packed_bit(data: &mut [u8], stride: u32, x: u32, y: u32) {
    let index = usize::try_from(y * stride + x / 8).unwrap_or(usize::MAX);
    if let Some(byte) = data.get_mut(index) {
        *byte |= 0x80 >> (x % 8);
    }
}

struct MonoCanvas {
    width: u32,
    height: u32,
    dpi: u16,
    stride: u32,
    data: Vec<u8>,
}

impl MonoCanvas {
    fn new(width: u32, height: u32, dpi: u16) -> RendererResult<Self> {
        let stride = width.div_ceil(8);
        let size = usize::try_from(stride)
            .ok()
            .and_then(|row| {
                usize::try_from(height)
                    .ok()
                    .and_then(|rows| row.checked_mul(rows))
            })
            .ok_or_else(|| RendererError::InvalidRasterPage("canvas size overflow".to_owned()))?;
        Ok(Self {
            width,
            height,
            dpi,
            stride,
            data: vec![0; size],
        })
    }

    fn fill_rect(&mut self, x: u32, y: u32, width: u32, height: u32) {
        let right = x.saturating_add(width).min(self.width);
        let bottom = y.saturating_add(height).min(self.height);
        for py in y.min(self.height)..bottom {
            for px in x.min(self.width)..right {
                set_packed_bit(&mut self.data, self.stride, px, py);
            }
        }
    }

    fn blit_receipt_image(
        &mut self,
        image: &ReceiptImage,
        x: u32,
        y: u32,
        target_width: u32,
        target_height: u32,
    ) {
        if target_width == 0 || target_height == 0 {
            return;
        }
        for target_y in 0..target_height {
            let source_y = target_y.saturating_mul(image.height) / target_height;
            for target_x in 0..target_width {
                let source_x = target_x.saturating_mul(image.width) / target_width;
                let index =
                    usize::try_from(source_y * image.stride + source_x / 8).unwrap_or(usize::MAX);
                if image
                    .data
                    .get(index)
                    .is_some_and(|byte| byte & (0x80 >> (source_x % 8)) != 0)
                {
                    set_packed_bit(
                        &mut self.data,
                        self.stride,
                        x.saturating_add(target_x),
                        y.saturating_add(target_y),
                    );
                }
            }
        }
    }

    fn into_page(self, dpi: u16) -> RasterPage {
        RasterPage {
            width: self.width,
            height: self.height,
            dpi_x: dpi,
            dpi_y: dpi,
            format: PixelFormat::Mono1,
            data: self.data,
        }
    }
}

#[cfg(test)]
mod tests {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use image::ImageEncoder;
    use oppa_protocol::{BarcodeFormat, ImageMediaType, PrintSection, TextAlignment};

    use super::{
        PageRenderOptions, PrintLayout, ReceiptElement, encode_raster_page_png,
        encode_raster_page_preview_png,
    };
    use crate::{DocumentRenderer, RasterDocument, RenderTarget, RendererError};

    fn representative_document() -> oppa_protocol::PrintDocument {
        let image = image::GrayImage::from_fn(8, 8, |x, y| {
            if x == y || x + y == 7 {
                image::Luma([0])
            } else {
                image::Luma([255])
            }
        });
        let mut png = std::io::Cursor::new(Vec::new());
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(
                image.as_raw(),
                image.width(),
                image.height(),
                image::ExtendedColorType::L8,
            )
            .expect("encode test image");
        oppa_protocol::PrintDocument {
            width: oppa_protocol::ReceiptWidth::Mm80,
            sections: vec![
                PrintSection::Text {
                    value: "Centered title".to_owned(),
                    align: Some(TextAlignment::Center),
                    bold: Some(true),
                },
                PrintSection::Row {
                    left: "Coffee".to_owned(),
                    right: "120.00".to_owned(),
                },
                PrintSection::Divider,
                PrintSection::Image {
                    media_type: ImageMediaType::Png,
                    data: STANDARD.encode(png.into_inner()),
                },
                PrintSection::Qr {
                    value: "https://openprinter.dev/test".to_owned(),
                },
                PrintSection::Barcode {
                    format: BarcodeFormat::Code128,
                    value: "RECEIPT-123".to_owned(),
                },
                PrintSection::Feed { lines: 2 },
                PrintSection::Cut,
            ],
        }
    }

    #[test]
    fn page_renderer_keeps_receipt_at_physical_width_and_centers_it_on_a4() {
        let document = representative_document();
        let rendered = DocumentRenderer::default()
            .render(
                &document,
                RenderTarget::Page(PageRenderOptions::A4_PORTRAIT),
            )
            .expect("page render");
        let crate::RenderedDocument::Raster(RasterDocument { pages }) = rendered else {
            panic!("expected raster page");
        };
        assert_eq!(pages.len(), 1);
        assert_eq!((pages[0].width, pages[0].height), (2480, 3508));
        assert_eq!(pages[0].dpi_x, 300);
        assert!(pages[0].data.iter().any(|byte| *byte != 0));
        let png = encode_raster_page_png(&pages[0]).expect("preview PNG");
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        let preview = encode_raster_page_preview_png(&pages[0]).expect("bounded preview PNG");
        let decoded_preview =
            image::load_from_memory_with_format(&preview, image::ImageFormat::Png)
                .expect("preview decodes");
        assert!(decoded_preview.width() <= 800);
        assert!(decoded_preview.height() <= 1_200);
    }

    #[test]
    fn office_cut_is_a_noop_and_all_graphical_sections_draw() {
        let document = representative_document();
        let layout = PrintLayout::from_document(&document).expect("layout");
        assert!(matches!(layout.elements.last(), Some(ReceiptElement::Cut)));
        let page = super::render_layout_page(&layout, PageRenderOptions::A4_PORTRAIT)
            .expect("page render");
        let mut without_cut = layout.clone();
        without_cut.elements.pop();
        let page_without_cut =
            super::render_layout_page(&without_cut, PageRenderOptions::A4_PORTRAIT)
                .expect("page render without device cut");
        assert_eq!(page, page_without_cut);
        assert!(page.pages[0].data.iter().any(|byte| *byte != 0));
        assert!(!page.pages[0].data.is_empty());
    }

    #[test]
    fn virtual_receipt_preview_is_narrow_and_contains_drawn_pixels() {
        let document = representative_document();
        let layout = PrintLayout::from_document(&document).expect("layout");
        let preview = super::render_layout_receipt(&layout, 203).expect("receipt preview");
        assert_eq!(preview.pages[0].width, 639);
        assert!(preview.pages[0].height < 2_000);
        assert!(preview.pages[0].data.iter().any(|byte| *byte != 0));
    }

    #[test]
    fn unsupported_page_resolution_is_rejected() {
        let error = super::render_layout_page(
            &PrintLayout {
                width: oppa_protocol::ReceiptWidth::Mm58,
                elements: vec![ReceiptElement::Cut],
            },
            PageRenderOptions {
                width_mm: 210,
                height_mm: 297,
                dpi: 1,
            },
        );
        assert!(matches!(error, Err(RendererError::InvalidPageOptions)));
    }
}
