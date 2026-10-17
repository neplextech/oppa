//! Validation and rasterization of untrusted PDF documents.
//!
//! PDF bytes are parsed by a memory-safe pure-Rust engine before any device
//! sees them. Validation resolves the page selection and rejects encrypted or
//! malformed files. Rasterization exists for backends that cannot submit PDF
//! natively; it applies the same print settings that native submission passes
//! to the operating system, so the two paths produce equivalent page geometry.
//!
//! Every stage is bounded: input bytes, page count, pixels per page, total
//! raster memory, and wall-clock time between pages. Parser or renderer panics
//! are caught at this boundary and reported as [`RendererError::PdfRender`].

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    time::Instant,
};

use hayro::{
    PixmapSettings, RenderCache, RenderSettings,
    hayro_interpret::InterpreterSettings,
    hayro_syntax::{LoadPdfError, Pdf, page::Page},
    render,
    vello_cpu::color::palette::css::WHITE,
};
use image::{RgbaImage, imageops};
use oppa_protocol::{
    MAX_PDF_DOCUMENT_BYTES, Orientation, PaperSize, PrintSettings, Scaling, StandardPaperSize,
};

use crate::{PixelFormat, RasterDocument, RasterPage, RendererError, RendererResult};

/// Maximum pages accepted in one source document.
pub const MAX_PDF_PAGES: usize = 2_000;
/// Maximum pages in one raster output, after copies are applied.
pub const MAX_PDF_RASTER_OUTPUT_PAGES: usize = 200;
/// Maximum packed raster bytes for one raster output.
pub const MAX_PDF_RASTER_BYTES: usize = 64 * 1024 * 1024;
/// Maximum pixels rendered or composed for one page.
pub const MAX_PDF_PIXELS_PER_PAGE: u64 = 42_000_000;
/// Default raster resolution for devices that need rasterized PDF pages.
pub const DEFAULT_PDF_RASTER_DPI: u16 = 300;

const MIN_RASTER_DPI: u16 = 72;
const MAX_RASTER_DPI: u16 = 600;
const MAX_PAGE_DIMENSION_POINTS: f32 = 14_400.0;
const POINTS_PER_INCH: f32 = 72.0;
const MILLIMETRES_PER_INCH: f64 = 25.4;
/// Luminance below this value becomes a black dot in one-bit output.
const MONOCHROME_THRESHOLD: u32 = 140;
const PDF_HEADER: &[u8] = b"%PDF-";
const HEADER_SEARCH_BYTES: usize = 1_024;

/// A validated PDF with its selected pages and the settings to apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfPrintDocument {
    /// Exact PDF bytes that passed validation.
    pub bytes: Vec<u8>,
    /// Print settings supplied with the document.
    pub settings: PrintSettings,
    /// Number of pages in the source document.
    pub page_count: usize,
    /// Zero-based indices of selected pages, in document order.
    pub selected_pages: Vec<usize>,
}

/// Raster conversion options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PdfRasterOptions {
    /// Output resolution in dots per inch, from 72 to 600.
    pub dpi: u16,
    /// Optional instant after which rasterization stops between pages.
    pub deadline: Option<Instant>,
}

impl Default for PdfRasterOptions {
    fn default() -> Self {
        Self {
            dpi: DEFAULT_PDF_RASTER_DPI,
            deadline: None,
        }
    }
}

/// Validates PDF bytes and resolves the requested page selection.
///
/// # Errors
///
/// Returns an error when the input exceeds the document limit, is not a
/// parseable PDF, is encrypted, has too many pages, or selects a missing page.
pub fn validate_pdf(bytes: Vec<u8>, settings: &PrintSettings) -> RendererResult<PdfPrintDocument> {
    if bytes.len() as u64 > MAX_PDF_DOCUMENT_BYTES {
        return Err(RendererError::PdfLimitExceeded {
            limit: "size in bytes",
            actual: bytes.len() as u64,
            maximum: MAX_PDF_DOCUMENT_BYTES,
        });
    }
    if !bytes
        .windows(PDF_HEADER.len())
        .take(HEADER_SEARCH_BYTES)
        .any(|window| window == PDF_HEADER)
    {
        return Err(RendererError::InvalidPdf(
            "the file does not start with a PDF header".to_owned(),
        ));
    }
    let page_count = {
        let pdf = load_pdf(bytes.clone())?;
        pdf.pages().len()
    };
    if page_count == 0 {
        return Err(RendererError::InvalidPdf(
            "the document contains no pages".to_owned(),
        ));
    }
    if page_count > MAX_PDF_PAGES {
        return Err(RendererError::PdfLimitExceeded {
            limit: "page count",
            actual: page_count as u64,
            maximum: MAX_PDF_PAGES as u64,
        });
    }
    let selected_pages = resolve_page_selection(settings.page_ranges.as_deref(), page_count)?;
    let output_pages = selected_pages.len() * usize::from(settings.copies());
    if output_pages > MAX_PDF_RASTER_OUTPUT_PAGES {
        return Err(RendererError::PdfLimitExceeded {
            limit: "output pages after copies",
            actual: output_pages as u64,
            maximum: MAX_PDF_RASTER_OUTPUT_PAGES as u64,
        });
    }
    Ok(PdfPrintDocument {
        bytes,
        settings: settings.clone(),
        page_count,
        selected_pages,
    })
}

/// Resolves a page-range expression such as `1-3,5` against a page count.
///
/// Absent ranges select every page. Overlapping ranges are merged, and the
/// result is always in document order.
///
/// # Errors
///
/// Returns [`RendererError::InvalidPageRange`] when the expression is malformed
/// or names a page beyond the document.
pub fn resolve_page_selection(
    ranges: Option<&str>,
    page_count: usize,
) -> RendererResult<Vec<usize>> {
    let Some(ranges) = ranges else {
        return Ok((0..page_count).collect());
    };
    oppa_protocol::validate_page_ranges(ranges)
        .map_err(|error| RendererError::InvalidPageRange(error.to_string()))?;
    let mut selected = vec![false; page_count];
    for part in ranges.split(',') {
        let (start, end) = part.split_once('-').unwrap_or((part, part));
        let start = parse_page(start)?;
        let end = parse_page(end)?;
        if end > page_count {
            return Err(RendererError::InvalidPageRange(format!(
                "page {end} does not exist; the document has {page_count} pages"
            )));
        }
        for page in start..=end {
            selected[page - 1] = true;
        }
    }
    Ok(selected
        .iter()
        .enumerate()
        .filter_map(|(index, chosen)| chosen.then_some(index))
        .collect())
}

fn parse_page(value: &str) -> RendererResult<usize> {
    value
        .parse::<usize>()
        .map_err(|_| RendererError::InvalidPageRange("page numbers must be integers".to_owned()))
}

/// Rasterizes the selected pages to one-bit device pages.
///
/// Copies repeat the whole selection in order. Orientation, paper size, and
/// scaling are applied per page exactly as the settings describe.
///
/// # Errors
///
/// Returns an error when parsing, a page exceeds the pixel budget, the total
/// raster exceeds its byte budget, or the deadline passes between pages.
pub fn rasterize_pdf(
    document: &PdfPrintDocument,
    options: PdfRasterOptions,
) -> RendererResult<RasterDocument> {
    if !(MIN_RASTER_DPI..=MAX_RASTER_DPI).contains(&options.dpi) {
        return Err(RendererError::InvalidPageOptions);
    }
    let pdf = load_pdf(document.bytes.clone())?;
    let pages = pdf.pages();
    let cache = RenderCache::new();
    let interpreter = InterpreterSettings::default();

    let copies = usize::from(document.settings.copies());
    let mut rendered = Vec::with_capacity(document.selected_pages.len());
    let mut selection_bytes = 0_usize;
    for &index in &document.selected_pages {
        check_deadline(options.deadline)?;
        let page = pages.get(index).ok_or_else(|| {
            RendererError::InvalidPageRange(format!("page {} is missing", index + 1))
        })?;
        let raster = render_page(page, options.dpi, &document.settings, &cache, &interpreter)?;
        selection_bytes = selection_bytes.saturating_add(raster.data.len());
        let output_bytes = selection_bytes.saturating_mul(copies);
        if output_bytes > MAX_PDF_RASTER_BYTES {
            return Err(RendererError::PdfLimitExceeded {
                limit: "raster bytes",
                actual: output_bytes as u64,
                maximum: MAX_PDF_RASTER_BYTES as u64,
            });
        }
        rendered.push(raster);
    }

    let mut output = Vec::with_capacity(rendered.len() * copies);
    for _ in 0..copies {
        check_deadline(options.deadline)?;
        output.extend(rendered.iter().cloned());
    }
    Ok(RasterDocument { pages: output })
}

fn check_deadline(deadline: Option<Instant>) -> RendererResult<()> {
    match deadline {
        Some(deadline) if Instant::now() >= deadline => Err(RendererError::RasterizationTimedOut),
        _ => Ok(()),
    }
}

fn load_pdf(bytes: Vec<u8>) -> RendererResult<Pdf> {
    catch_unwind(AssertUnwindSafe(|| Pdf::new(bytes)))
        .map_err(|_| RendererError::PdfRender("the PDF parser stopped unexpectedly".to_owned()))?
        .map_err(|error| match error {
            LoadPdfError::Decryption(_) => RendererError::EncryptedPdf,
            LoadPdfError::Invalid => {
                RendererError::InvalidPdf("the file could not be parsed".to_owned())
            }
        })
}

fn render_page<'a>(
    page: &'a Page<'a>,
    dpi: u16,
    settings: &PrintSettings,
    cache: &RenderCache<'a>,
    interpreter: &InterpreterSettings,
) -> RendererResult<RasterPage> {
    let (width_points, height_points) = page.render_dimensions();
    if !(width_points > 0.0
        && height_points > 0.0
        && width_points <= MAX_PAGE_DIMENSION_POINTS
        && height_points <= MAX_PAGE_DIMENSION_POINTS)
    {
        return Err(RendererError::PdfLimitExceeded {
            limit: "page dimension in points",
            actual: width_points.max(height_points) as u64,
            maximum: MAX_PAGE_DIMENSION_POINTS as u64,
        });
    }
    let scale = f32::from(dpi) / POINTS_PER_INCH;
    check_pixel_budget(
        (width_points * scale).ceil() as u32,
        (height_points * scale).ceil() as u32,
    )?;

    let pixmap = catch_unwind(AssertUnwindSafe(|| {
        render(
            page,
            cache,
            interpreter,
            &RenderSettings::default(),
            &PixmapSettings {
                x_scale: scale,
                y_scale: scale,
                bg_color: WHITE,
            },
        )
    }))
    .map_err(|_| RendererError::PdfRender("the PDF renderer stopped unexpectedly".to_owned()))?;

    let mut content = RgbaImage::from_raw(
        u32::from(pixmap.width()),
        u32::from(pixmap.height()),
        pixmap.data_as_u8_slice().to_vec(),
    )
    .ok_or_else(|| {
        RendererError::PdfRender("the renderer returned an invalid bitmap".to_owned())
    })?;

    // The PDF geometry is authoritative: hayro truncates pixmap dimensions, so
    // the canonical size is recomputed here and the bitmap is resampled to it.
    let wants_landscape = settings
        .orientation
        .map(|orientation| orientation == Orientation::Landscape);
    let (mut page_width, mut page_height) = (
        (width_points * scale).round().max(1.0) as u32,
        (height_points * scale).round().max(1.0) as u32,
    );
    if let Some(wants_landscape) = wants_landscape
        && wants_landscape != (page_width > page_height)
    {
        content = imageops::rotate90(&content);
        std::mem::swap(&mut page_width, &mut page_height);
    }

    let target = settings
        .paper
        .map(|paper| target_pixels(paper, settings.orientation, dpi))
        .transpose()?;
    let factor = scaling_factor(settings.scaling, (page_width, page_height), target);
    let scaled_width = scaled_dimension(page_width, factor);
    let scaled_height = scaled_dimension(page_height, factor);
    if content.dimensions() != (scaled_width, scaled_height) {
        content = imageops::resize(
            &content,
            scaled_width,
            scaled_height,
            imageops::FilterType::Lanczos3,
        );
    }

    let (canvas_width, canvas_height) = target.unwrap_or((scaled_width, scaled_height));
    check_pixel_budget(canvas_width, canvas_height)?;
    let mut canvas = RgbaImage::from_pixel(canvas_width, canvas_height, image::Rgba([255; 4]));
    let (offset_x, offset_y) = if target.is_some() {
        (
            i64::from(canvas_width) / 2 - i64::from(scaled_width) / 2,
            i64::from(canvas_height) / 2 - i64::from(scaled_height) / 2,
        )
    } else {
        (0, 0)
    };
    imageops::overlay(&mut canvas, &content, offset_x, offset_y);

    Ok(RasterPage {
        width: canvas_width,
        height: canvas_height,
        dpi_x: dpi,
        dpi_y: dpi,
        format: PixelFormat::Mono1,
        data: pack_monochrome(&canvas),
    })
}

fn check_pixel_budget(width: u32, height: u32) -> RendererResult<()> {
    if width == 0 || height == 0 {
        return Err(RendererError::InvalidPageOptions);
    }
    let pixels = u64::from(width) * u64::from(height);
    if pixels > MAX_PDF_PIXELS_PER_PAGE {
        return Err(RendererError::PdfLimitExceeded {
            limit: "pixels per page",
            actual: pixels,
            maximum: MAX_PDF_PIXELS_PER_PAGE,
        });
    }
    Ok(())
}

fn target_pixels(
    paper: PaperSize,
    orientation: Option<Orientation>,
    dpi: u16,
) -> RendererResult<(u32, u32)> {
    let (mut width_mm, mut height_mm) = paper_millimetres(paper);
    match orientation {
        Some(Orientation::Landscape) if width_mm < height_mm => {
            std::mem::swap(&mut width_mm, &mut height_mm);
        }
        Some(Orientation::Portrait) if width_mm > height_mm => {
            std::mem::swap(&mut width_mm, &mut height_mm);
        }
        _ => {}
    }
    let width = millimetres_to_pixels(width_mm, dpi);
    let height = millimetres_to_pixels(height_mm, dpi);
    check_pixel_budget(width, height)?;
    Ok((width, height))
}

fn paper_millimetres(paper: PaperSize) -> (f64, f64) {
    match paper {
        PaperSize::Standard(StandardPaperSize::A4) => (210.0, 297.0),
        PaperSize::Standard(StandardPaperSize::A5) => (148.0, 210.0),
        PaperSize::Standard(StandardPaperSize::Letter) => (215.9, 279.4),
        PaperSize::Standard(StandardPaperSize::Legal) => (215.9, 355.6),
        PaperSize::Custom(custom) => (f64::from(custom.width_mm), f64::from(custom.height_mm)),
    }
}

fn millimetres_to_pixels(millimetres: f64, dpi: u16) -> u32 {
    (millimetres * f64::from(dpi) / MILLIMETRES_PER_INCH).round() as u32
}

fn scaling_factor(
    scaling: Option<Scaling>,
    (page_width, page_height): (u32, u32),
    target: Option<(u32, u32)>,
) -> f32 {
    match scaling {
        None | Some(Scaling::Actual) => 1.0,
        Some(Scaling::Percent(percent)) => f32::from(percent) / 100.0,
        Some(Scaling::Fit) => target.map_or(1.0, |(width, height)| {
            let horizontal = width as f32 / page_width as f32;
            let vertical = height as f32 / page_height as f32;
            horizontal.min(vertical).min(1.0)
        }),
    }
}

fn scaled_dimension(value: u32, factor: f32) -> u32 {
    ((value as f32 * factor).round() as u32).max(1)
}

fn pack_monochrome(image: &RgbaImage) -> Vec<u8> {
    let width = image.width();
    let bytes_per_row = width.div_ceil(8) as usize;
    let mut data = vec![0_u8; bytes_per_row * image.height() as usize];
    for (y, row) in image.rows().enumerate() {
        for (x, pixel) in row.enumerate() {
            let [red, green, blue, _] = pixel.0;
            let luminance =
                (u32::from(red) * 299 + u32::from(green) * 587 + u32::from(blue) * 114) / 1000;
            if luminance < MONOCHROME_THRESHOLD {
                let index = y * bytes_per_row + x / 8;
                data[index] |= 0x80 >> (x % 8);
            }
        }
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_defaults_to_every_page_in_order() {
        assert_eq!(resolve_page_selection(None, 3).unwrap(), vec![0, 1, 2]);
    }

    #[test]
    fn selection_merges_overlaps_and_keeps_document_order() {
        assert_eq!(
            resolve_page_selection(Some("3,1-2,2"), 4).unwrap(),
            vec![0, 1, 2]
        );
    }

    #[test]
    fn selection_rejects_pages_beyond_the_document() {
        let error = resolve_page_selection(Some("1-9"), 3).unwrap_err();
        assert!(matches!(error, RendererError::InvalidPageRange(_)));
    }

    #[test]
    fn validation_rejects_non_pdf_bytes_before_parsing() {
        let error = validate_pdf(b"not a document".to_vec(), &PrintSettings::default())
            .expect_err("plain text is not a PDF");
        assert!(matches!(error, RendererError::InvalidPdf(_)));
    }

    #[test]
    fn validation_rejects_oversized_input() {
        let bytes = vec![b'%'; usize::try_from(MAX_PDF_DOCUMENT_BYTES).unwrap() + 1];
        let error = validate_pdf(bytes, &PrintSettings::default()).expect_err("too large");
        assert!(matches!(error, RendererError::PdfLimitExceeded { .. }));
    }

    #[test]
    fn page_range_syntax_is_checked_before_resolution() {
        let error = resolve_page_selection(Some("3-1"), 5).unwrap_err();
        assert!(matches!(error, RendererError::InvalidPageRange(_)));
    }

    #[test]
    fn monochrome_packing_sets_black_pixels_most_significant_bit_first() {
        let mut image = RgbaImage::from_pixel(9, 1, image::Rgba([255, 255, 255, 255]));
        image.put_pixel(0, 0, image::Rgba([0, 0, 0, 255]));
        image.put_pixel(8, 0, image::Rgba([0, 0, 0, 255]));
        assert_eq!(pack_monochrome(&image), vec![0b1000_0000, 0b1000_0000]);
    }

    #[test]
    fn landscape_target_swaps_standard_paper_dimensions() {
        let (width, height) = target_pixels(
            PaperSize::Standard(StandardPaperSize::A4),
            Some(Orientation::Landscape),
            300,
        )
        .unwrap();
        assert!(width > height);
        assert_eq!((width, height), (3508, 2480));
    }

    #[test]
    fn fit_scaling_never_enlarges_a_page() {
        assert!(
            (scaling_factor(Some(Scaling::Fit), (100, 100), Some((1_000, 1_000))) - 1.0).abs()
                < f32::EPSILON
        );
        assert!(
            (scaling_factor(Some(Scaling::Fit), (100, 100), Some((50, 200))) - 0.5).abs()
                < f32::EPSILON
        );
    }
}
