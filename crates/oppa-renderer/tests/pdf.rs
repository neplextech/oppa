//! Integration tests for PDF validation and rasterization against a real parser.

use std::time::{Duration, Instant};

use oppa_protocol::{
    CustomPaperSize, Orientation, PaperSize, PrintSettings, Scaling, StandardPaperSize,
};
use oppa_renderer::{
    DocumentRenderer, PdfPrintDocument, PdfRasterOptions, PixelFormat, RasterDocument,
    RenderedDocument, RendererError, rasterize_pdf, validate_pdf,
};

const THREE_PAGES: &[u8] = include_bytes!("fixtures/three-pages.pdf");

fn document(settings: &PrintSettings) -> PdfPrintDocument {
    validate_pdf(THREE_PAGES.to_vec(), settings).expect("fixture must validate")
}

fn raster(settings: PrintSettings) -> RasterDocument {
    rasterize_pdf(
        &document(&settings),
        PdfRasterOptions {
            dpi: 300,
            deadline: None,
        },
    )
    .expect("fixture must rasterize")
}

#[test]
fn validation_reports_every_page_by_default() {
    let document = document(&PrintSettings::default());
    assert_eq!(document.page_count, 3);
    assert_eq!(document.selected_pages, vec![0, 1, 2]);
}

#[test]
fn page_ranges_select_pages_in_document_order() {
    let document = document(&PrintSettings {
        page_ranges: Some("3,1".to_owned()),
        ..PrintSettings::default()
    });
    assert_eq!(document.selected_pages, vec![0, 2]);
}

#[test]
fn page_ranges_beyond_the_document_are_rejected() {
    let error = validate_pdf(
        THREE_PAGES.to_vec(),
        &PrintSettings {
            page_ranges: Some("2-4".to_owned()),
            ..PrintSettings::default()
        },
    )
    .expect_err("page four does not exist");
    assert!(matches!(error, RendererError::InvalidPageRange(_)));
}

#[test]
fn truncated_documents_are_rejected_without_panicking() {
    let truncated = THREE_PAGES[..THREE_PAGES.len() / 3].to_vec();
    let result = validate_pdf(truncated, &PrintSettings::default());
    assert!(
        result.is_err(),
        "a truncated PDF must not validate as complete"
    );
}

#[test]
fn default_raster_uses_each_page_size_at_300_dpi() {
    let raster = raster(PrintSettings::default());
    assert_eq!(raster.pages.len(), 3);
    let first = &raster.pages[0];
    assert_eq!((first.width, first.height), (2480, 3508));
    assert_eq!(first.format, PixelFormat::Mono1);
    assert!(
        first.data.iter().any(|byte| *byte != 0),
        "text must produce ink"
    );
    first.validate().expect("raster page must be well-formed");
}

#[test]
fn copies_repeat_the_whole_selection_in_order() {
    let raster = raster(PrintSettings {
        copies: Some(2),
        page_ranges: Some("1-2".to_owned()),
        ..PrintSettings::default()
    });
    let sizes: Vec<(u32, u32)> = raster
        .pages
        .iter()
        .map(|page| (page.width, page.height))
        .collect();
    assert_eq!(sizes.len(), 4);
    assert_eq!(sizes[0], sizes[2]);
    assert_eq!(sizes[1], sizes[3]);
}

#[test]
fn landscape_orientation_rotates_portrait_pages() {
    let raster = raster(PrintSettings {
        orientation: Some(Orientation::Landscape),
        page_ranges: Some("1".to_owned()),
        ..PrintSettings::default()
    });
    let page = &raster.pages[0];
    assert!(
        page.width > page.height,
        "landscape pages must be wider than tall"
    );
}

#[test]
fn a5_paper_with_fit_scaling_places_content_on_the_target_size() {
    let raster = raster(PrintSettings {
        paper: Some(PaperSize::Standard(StandardPaperSize::A5)),
        scaling: Some(Scaling::Fit),
        page_ranges: Some("1".to_owned()),
        ..PrintSettings::default()
    });
    let page = &raster.pages[0];
    assert_eq!((page.width, page.height), (1748, 2480));
}

#[test]
fn custom_label_media_is_exact_in_pixels() {
    let raster = raster(PrintSettings {
        paper: Some(PaperSize::Custom(CustomPaperSize {
            width_mm: 50,
            height_mm: 30,
        })),
        scaling: Some(Scaling::Fit),
        page_ranges: Some("1".to_owned()),
        ..PrintSettings::default()
    });
    let page = &raster.pages[0];
    assert_eq!((page.width, page.height), (591, 354));
    assert_eq!((page.dpi_x, page.dpi_y), (300, 300));
}

#[test]
fn percent_scaling_shrinks_the_content_without_changing_the_paper() {
    let raster = raster(PrintSettings {
        paper: Some(PaperSize::Standard(StandardPaperSize::A4)),
        scaling: Some(Scaling::Percent(50)),
        page_ranges: Some("1".to_owned()),
        ..PrintSettings::default()
    });
    assert_eq!(
        (raster.pages[0].width, raster.pages[0].height),
        (2480, 3508)
    );
}

#[test]
fn an_expired_deadline_stops_rasterization_between_pages() {
    let document = document(&PrintSettings::default());
    let error = rasterize_pdf(
        &document,
        PdfRasterOptions {
            dpi: 300,
            deadline: Some(Instant::now() - Duration::from_secs(1)),
        },
    )
    .expect_err("expired deadline must stop work");
    assert!(matches!(error, RendererError::RasterizationTimedOut));
}

#[test]
fn unsupported_resolution_is_rejected() {
    let document = document(&PrintSettings::default());
    let error = rasterize_pdf(
        &document,
        PdfRasterOptions {
            dpi: 1200,
            deadline: None,
        },
    )
    .expect_err("1200 DPI exceeds the limit");
    assert!(matches!(error, RendererError::InvalidPageOptions));
}

#[test]
fn document_renderer_returns_a_pdf_variant_with_its_settings() {
    let settings = PrintSettings {
        copies: Some(2),
        ..PrintSettings::default()
    };
    let rendered = DocumentRenderer::default()
        .render_pdf(THREE_PAGES.to_vec(), &settings)
        .expect("valid PDF");
    let RenderedDocument::Pdf(document) = rendered else {
        panic!("PDF input must render as the PDF variant");
    };
    assert_eq!(document.settings.copies(), 2);
    assert_eq!(document.bytes, THREE_PAGES);
}

#[test]
fn invalid_settings_fail_before_parsing_the_document() {
    let settings = PrintSettings {
        copies: Some(0),
        ..PrintSettings::default()
    };
    let error = DocumentRenderer::default()
        .render_pdf(THREE_PAGES.to_vec(), &settings)
        .expect_err("zero copies is invalid");
    assert!(matches!(error, RendererError::InvalidDocument(_)));
}

#[test]
fn raster_output_beyond_the_byte_budget_is_rejected_before_copies_are_built() {
    let document = document(&PrintSettings {
        copies: Some(22),
        ..PrintSettings::default()
    });
    let error = rasterize_pdf(
        &document,
        PdfRasterOptions {
            dpi: 300,
            deadline: None,
        },
    )
    .expect_err("66 A4 pages at 300 DPI exceed the raster budget");
    assert!(matches!(
        error,
        RendererError::PdfLimitExceeded {
            limit: "raster bytes",
            ..
        }
    ));
}
