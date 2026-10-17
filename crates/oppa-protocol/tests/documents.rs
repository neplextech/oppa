//! Backward compatibility and validation of receipt and PDF document representations.

use oppa_protocol::{
    DocumentFormat, Duplex, JobDocument, PaperSize, PdfDocument, PrintJob, PrintSettings,
    PrinterCapabilities, Scaling, ServerMessageKind, StandardPaperSize, Validate,
    decode_server_message, encode_server_message, validate_document_url, validate_page_ranges,
};
use serde_json::{Value, json};

const SHA256: &str = "9f2c7e1a4b8d03e5f6a7b8c9d0e1f2a34b5c6d7e8f9a0b1c2d3e4f5a6b7c8d9e";

fn job_with_document(document: Value) -> Value {
    json!({
        "protocolVersion": "1",
        "messageId": "msg_doc_001",
        "sentAt": "2026-07-28T10:02:00Z",
        "type": "server.print_job",
        "payload": {
            "jobId": "job_doc_001",
            "idempotencyKey": "doc_001_v1",
            "printerId": "printer_001",
            "createdAt": "2026-07-28T10:01:58Z",
            "document": document
        }
    })
}

fn decode_job(document: Value) -> Result<PrintJob, oppa_protocol::ProtocolError> {
    let raw = serde_json::to_vec(&job_with_document(document)).expect("serialize fixture");
    match decode_server_message(&raw)?.kind {
        ServerMessageKind::PrintJob(job) => Ok(job),
        other => panic!("expected a print job, got {other:?}"),
    }
}

fn pdf_document(source: Value, settings: Option<Value>) -> Value {
    let mut document = json!({"format": "pdf", "source": source});
    if let Some(settings) = settings {
        document["settings"] = settings;
    }
    document
}

fn https_source() -> Value {
    json!({
        "type": "url",
        "url": "https://documents.example.com/v1/documents/doc_1",
        "sha256": SHA256,
        "byteLength": 4096
    })
}

#[test]
fn legacy_receipts_decode_as_receipts_and_keep_their_wire_shape() {
    let legacy = json!({
        "width": 80,
        "sections": [{"type": "text", "value": "Thank you"}, {"type": "cut"}]
    });
    let job = decode_job(legacy.clone()).expect("legacy receipt must decode");
    assert!(matches!(job.document, JobDocument::Receipt(_)));

    let encoded = serde_json::to_value(&job).expect("encode job");
    assert_eq!(
        encoded["document"], legacy,
        "receipts must not gain a format member"
    );
}

#[test]
fn pdf_documents_are_selected_by_their_format_member() {
    let job = decode_job(pdf_document(https_source(), None)).expect("pdf job must decode");
    let JobDocument::Pdf(document) = &job.document else {
        panic!("a format member selects the PDF representation");
    };
    assert_eq!(job.document.format_name(), "pdf");
    assert!(document.settings.is_none());
}

#[test]
fn pdf_documents_round_trip_through_the_canonical_codec() {
    let settings = json!({
        "copies": 2,
        "paper": {"widthMm": 50, "heightMm": 30},
        "orientation": "landscape",
        "scaling": {"percent": 80},
        "pageRanges": "1-2,4",
        "duplex": "long-edge"
    });
    let raw = serde_json::to_vec(&job_with_document(pdf_document(
        https_source(),
        Some(settings.clone()),
    )))
    .expect("serialize fixture");
    let message = decode_server_message(&raw).expect("valid pdf job");
    let encoded: Value = serde_json::from_slice(&encode_server_message(&message).expect("encode"))
        .expect("encoded JSON");
    assert_eq!(encoded["payload"]["document"]["settings"], settings);
    assert_eq!(encoded["payload"]["document"]["format"], "pdf");
}

#[test]
fn unknown_document_formats_are_rejected() {
    assert!(
        decode_job(json!({"format": "postscript", "source": https_source()})).is_err(),
        "unsupported formats must be rejected"
    );
}

#[test]
fn inline_bytes_are_not_a_valid_pdf_source() {
    let error = decode_job(pdf_document(
        json!({"type": "inline", "data": "JVBERi0xLjQ="}),
        None,
    ));
    assert!(
        error.is_err(),
        "bytes must never travel inside a job message"
    );
}

#[test]
fn only_https_or_loopback_document_urls_are_accepted() {
    assert!(validate_document_url("https://documents.example.com/v1/doc", "url").is_ok());
    assert!(validate_document_url("http://127.0.0.1:8787/v1/doc", "url").is_ok());
    assert!(validate_document_url("http://localhost/doc", "url").is_ok());
    for rejected in [
        "http://documents.example.com/doc",
        "https://user:secret@documents.example.com/doc",
        "https://documents.example.com/doc#fragment",
        "https://documents.example.com/has space",
        "https://[2001:db8::1]/doc",
        "ftp://documents.example.com/doc",
        "/relative/path",
    ] {
        assert!(
            validate_document_url(rejected, "url").is_err(),
            "{rejected} must be rejected"
        );
    }
}

#[test]
fn document_urls_are_length_bounded() {
    let long = format!("https://documents.example.com/{}", "a".repeat(2_100));
    assert!(validate_document_url(&long, "url").is_err());
}

#[test]
fn sha256_and_byte_length_are_strict() {
    let uppercase = json!({
        "type": "url",
        "url": "https://documents.example.com/doc",
        "sha256": SHA256.to_uppercase(),
        "byteLength": 1
    });
    assert!(decode_job(pdf_document(uppercase, None)).is_err());

    let too_large = json!({
        "type": "url",
        "url": "https://documents.example.com/doc",
        "sha256": SHA256,
        "byteLength": 20 * 1024 * 1024 + 1
    });
    assert!(decode_job(pdf_document(too_large, None)).is_err());
}

#[test]
fn page_ranges_must_ascend_and_start_at_one() {
    assert!(validate_page_ranges("1-3,5,7-7").is_ok());
    assert!(validate_page_ranges("5-2").is_err());
    assert!(validate_page_ranges("0-2").is_err());
    assert!(validate_page_ranges("1-2-3").is_err());
    assert!(validate_page_ranges("").is_err());
    assert!(validate_page_ranges("100000").is_err());
}

#[test]
fn settings_bounds_are_enforced_before_rendering() {
    let too_many_copies = PrintSettings {
        copies: Some(100),
        ..PrintSettings::default()
    };
    assert_eq!(
        too_many_copies.validate().expect_err("copies").path,
        "copies"
    );

    let tiny_label = PrintSettings {
        paper: Some(PaperSize::Custom(oppa_protocol::CustomPaperSize {
            width_mm: 5,
            height_mm: 30,
        })),
        ..PrintSettings::default()
    };
    assert!(tiny_label.validate().is_err());

    let extreme_scale = PrintSettings {
        scaling: Some(Scaling::Percent(500)),
        ..PrintSettings::default()
    };
    assert!(extreme_scale.validate().is_err());
}

#[test]
fn print_settings_defaults_are_documented_values() {
    let settings = PrintSettings::default();
    assert_eq!(settings.copies(), 1);
    assert_eq!(settings.duplex(), Duplex::Simplex);
    let standard = PrintSettings {
        paper: Some(PaperSize::Standard(StandardPaperSize::Letter)),
        ..PrintSettings::default()
    };
    assert_eq!(
        serde_json::to_value(&standard).expect("serialize settings")["paper"],
        "letter"
    );
}

#[test]
fn pdf_document_rejects_unknown_fields() {
    let mut document = pdf_document(https_source(), None);
    document["extra"] = json!(true);
    let parsed: Result<PdfDocument, _> = serde_json::from_value(document);
    assert!(parsed.is_err());
}

#[test]
fn printer_capabilities_without_document_fields_still_decode() {
    let legacy = json!({
        "mediaWidths": [58, 80],
        "systemDriver": true,
        "raster": true,
        "cut": false,
        "qr": true,
        "barcode": true
    });
    let capabilities: PrinterCapabilities =
        serde_json::from_value(legacy).expect("legacy capabilities must decode");
    assert_eq!(capabilities.document_formats, None);
    assert_eq!(capabilities.print_options, None);
    capabilities
        .validate()
        .expect("legacy capabilities validate");
    let encoded = serde_json::to_value(&capabilities).expect("encode");
    assert!(
        encoded.get("documentFormats").is_none(),
        "absent fields stay absent"
    );
}

#[test]
fn printer_capabilities_advertise_document_formats_and_options() {
    let advertised = json!({
        "mediaWidths": [80],
        "raster": true,
        "cut": false,
        "qr": false,
        "barcode": false,
        "documentFormats": ["receipt", "pdf"],
        "paperSizes": ["a4", "letter"],
        "customPaperSize": true,
        "printOptions": {
            "copies": true,
            "duplex": false,
            "orientation": true,
            "scaling": true,
            "pageRanges": true
        }
    });
    let capabilities: PrinterCapabilities =
        serde_json::from_value(advertised).expect("advertised capabilities decode");
    capabilities
        .validate()
        .expect("advertised capabilities validate");
    assert_eq!(
        capabilities.document_formats,
        Some(vec![DocumentFormat::Receipt, DocumentFormat::Pdf])
    );
    assert_eq!(
        capabilities.paper_sizes,
        Some(vec![StandardPaperSize::A4, StandardPaperSize::Letter])
    );
}

#[test]
fn duplicate_document_formats_are_rejected() {
    let duplicated = json!({
        "mediaWidths": [80],
        "raster": true,
        "cut": false,
        "qr": false,
        "barcode": false,
        "documentFormats": ["pdf", "pdf"]
    });
    let capabilities: PrinterCapabilities =
        serde_json::from_value(duplicated).expect("shape is valid");
    assert!(capabilities.validate().is_err());
}

#[test]
fn receipt_jobs_still_validate_through_the_job_document_union() {
    let job = decode_job(json!({
        "width": 58,
        "sections": [{"type": "divider"}]
    }))
    .expect("receipt");
    job.validate().expect("job validates");
}
