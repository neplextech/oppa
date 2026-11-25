//! Regression coverage and optional real-raster visual review fixtures.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use oppa_protocol::{
    BarcodeFormat, ImageMediaType, PrintDocument, PrintSection, ReceiptWidth, TextAlignment,
};
use oppa_renderer::{
    DocumentRenderer, EscPosInterpreter, PrintLayout, ReceiptElement, ReceiptPaperEvent,
    RenderTarget, RenderedDocument, encode_raster_page_png, render_layout_receipt,
    render_layout_receipt_with_events,
};

#[test]
fn physical_paper_preserves_blank_margins_and_encoder_columns() {
    for (width, columns, paper_width, printable) in [
        (ReceiptWidth::Mm58, 32, 464, 384),
        (ReceiptWidth::Mm80, 48, 639, 576),
    ] {
        let layout = PrintLayout {
            width,
            elements: vec![ReceiptElement::Divider],
        };
        let page = render_layout_receipt(&layout, 203).unwrap().pages.remove(0);
        assert_eq!(page.width, paper_width);
        let png = encode_raster_page_png(&page).unwrap();
        let gray = image::load_from_memory(&png).unwrap().to_luma8();
        let left = (paper_width - printable) / 2;
        for (x, _, pixel) in gray.enumerate_pixels() {
            assert!(pixel[0] == 0 || pixel[0] == 255);
            if x < left || x >= left + printable {
                assert_eq!(pixel[0], 255);
            }
        }
        assert_eq!(
            gray.pixels().filter(|p| p[0] == 0).count(),
            printable as usize
        );
        let text = |length| PrintLayout {
            width,
            elements: vec![ReceiptElement::Text {
                value: "M".repeat(length),
                alignment: TextAlignment::Left,
                bold: false,
            }],
        };
        assert_eq!(
            render_layout_receipt(&text(columns), 203).unwrap().pages[0].height,
            24
        );
        assert_eq!(
            render_layout_receipt(&text(columns + 1), 203)
                .unwrap()
                .pages[0]
                .height,
            48
        );
        for dpi in [72, 203, 300, 600] {
            let raster = render_layout_receipt(&text(columns), dpi).unwrap();
            assert_eq!(raster.pages[0].height, 24 * u32::from(dpi) / 203);
        }
    }
}

#[test]
fn feed_and_cut_coordinates_come_from_the_raster_pass_and_cut_adds_no_ink() {
    let mut layout = PrintLayout {
        width: ReceiptWidth::Mm58,
        elements: vec![
            ReceiptElement::Divider,
            ReceiptElement::Feed { lines: 2 },
            ReceiptElement::Cut,
        ],
    };
    let (with_cut, events) = render_layout_receipt_with_events(&layout, 203).unwrap();
    assert_eq!(
        events,
        vec![
            ReceiptPaperEvent::Feed {
                y: 24,
                height: 48,
                lines: 2
            },
            ReceiptPaperEvent::Cut { y: 72 }
        ]
    );
    layout.elements.pop();
    assert_eq!(with_cut, render_layout_receipt(&layout, 203).unwrap());
    assert_eq!(with_cut.pages[0].height, 72);
}

#[test]
fn real_escpos_visual_fixtures_keep_images_symbols_and_long_paper() {
    let directory = std::env::var_os("OPPA_RECEIPT_FIXTURES").map(std::path::PathBuf::from);
    if let Some(path) = &directory {
        std::fs::create_dir_all(path).unwrap();
    }
    for (name, width, lines) in [
        ("short58", ReceiptWidth::Mm58, 2),
        ("short80", ReceiptWidth::Mm80, 2),
        ("long80", ReceiptWidth::Mm80, 85),
        ("symbols58", ReceiptWidth::Mm58, 2),
    ] {
        let mut sections = vec![
            PrintSection::Text {
                value: "OPENPRINTER".into(),
                align: Some(TextAlignment::Center),
                bold: Some(true),
            },
            PrintSection::Text {
                value: "Virtual print sample".into(),
                align: Some(TextAlignment::Center),
                bold: None,
            },
            PrintSection::Divider,
        ];
        for i in 0..lines {
            sections.push(PrintSection::Row {
                left: format!("Sample item {}", i + 1),
                right: "12.50".into(),
            });
        }
        if name == "symbols58" {
            let mut logo = image::GrayImage::from_pixel(96, 32, image::Luma([255]));
            for y in 0..32 {
                for x in 0..96 {
                    if x / 8 % 2 == y / 8 % 2 {
                        logo.put_pixel(x, y, image::Luma([0]));
                    }
                }
            }
            let mut png = std::io::Cursor::new(Vec::new());
            logo.write_to(&mut png, image::ImageFormat::Png).unwrap();
            sections.push(PrintSection::Image {
                media_type: ImageMediaType::Png,
                data: STANDARD.encode(png.into_inner()),
            });
            sections.push(PrintSection::Qr {
                value: "https://openprinter.dev/test".into(),
            });
            sections.push(PrintSection::Barcode {
                format: BarcodeFormat::Code128,
                value: "TEST-123".into(),
            });
        }
        sections.extend([
            PrintSection::Divider,
            PrintSection::Row {
                left: "Total".into(),
                right: "25.00".into(),
            },
            PrintSection::Feed { lines: 3 },
            PrintSection::Cut,
        ]);
        let document = PrintDocument { width, sections };
        let RenderedDocument::EscPos(encoded) = DocumentRenderer::default()
            .render(&document, RenderTarget::EscPos)
            .unwrap()
        else {
            panic!("ESC/POS expected")
        };
        let original = encoded.bytes.clone();
        let interpreted = EscPosInterpreter::new(width)
            .interpret(&encoded.bytes)
            .unwrap();
        let (raster, events) = render_layout_receipt_with_events(&interpreted.layout, 203).unwrap();
        assert_eq!(encoded.bytes, original);
        let page = &raster.pages[0];
        assert!(page.data.iter().any(|byte| *byte != 0));
        if name == "symbols58" {
            assert_eq!(
                (
                    interpreted.diagnostics.images,
                    interpreted.diagnostics.qr_codes,
                    interpreted.diagnostics.barcodes
                ),
                (1, 1, 1)
            );
        }
        if name == "long80" {
            assert!(page.height > 2000);
        }
        if let Some(path) = &directory {
            std::fs::write(
                path.join(format!("{name}.png")),
                encode_raster_page_png(page).unwrap(),
            )
            .unwrap();
            std::fs::write(path.join(format!("{name}.json")), serde_json::to_vec(&serde_json::json!({ "byteLength": encoded.bytes.len(), "width": page.width, "height": page.height, "dpiX": page.dpi_x, "dpiY": page.dpi_y, "paperEvents": events })).unwrap()).unwrap();
        }
    }
}
