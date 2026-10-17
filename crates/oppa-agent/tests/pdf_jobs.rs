//! PDF job lifecycle tests with a scripted document fetcher and virtual printers.

use std::sync::Arc;

use async_trait::async_trait;
use oppa_agent::{
    Agent, AgentBuilder, OutboundReportError, OutboundReporter, ProcessOutcome,
    StaticPrinterResolver,
};
use oppa_core::{PrintJobId, PrinterId};
use oppa_documents::sha256_hex;
use oppa_documents::{DocumentFetchError, DocumentFetcher};
use oppa_printer::{
    PrinterConnection, PrinterLanguage, PrinterRef, SubmissionMode, VirtualPrinterProfile,
};
use oppa_protocol::{
    AgentMessage, AgentMessageKind, Duplex, JobDocument, PdfDocument, PdfFormat, PdfSource,
    PrintJob, PrintSettings, ServerMessage, ServerMessageKind,
};
use oppa_renderer::RenderedDocument;
use oppa_spooler::{SpoolerRegistry, VirtualOutcome, VirtualSimulation, VirtualSpooler};
use oppa_storage::{JobRepository, SqliteStorage};
use tokio::sync::Mutex;

const PRINTER_ID: &str = "printer_office_pdf";
const PDF: &[u8] = include_bytes!("../../oppa-renderer/tests/fixtures/three-pages.pdf");

#[derive(Clone)]
enum FetchBehaviour {
    Serve(Vec<u8>),
    Fail(fn() -> DocumentFetchError),
}

struct ScriptedFetcher {
    behaviour: FetchBehaviour,
}

#[async_trait]
impl DocumentFetcher for ScriptedFetcher {
    async fn fetch_pdf(
        &self,
        _job_id: &str,
        _source: &PdfSource,
    ) -> Result<Vec<u8>, DocumentFetchError> {
        match &self.behaviour {
            FetchBehaviour::Serve(bytes) => Ok(bytes.clone()),
            FetchBehaviour::Fail(error) => Err(error()),
        }
    }
}

#[derive(Default)]
struct Reporter {
    messages: Mutex<Vec<AgentMessage>>,
}

#[async_trait]
impl OutboundReporter for Reporter {
    async fn report(&self, message: &AgentMessage) -> Result<(), OutboundReportError> {
        self.messages.lock().await.push(message.clone());
        Ok(())
    }
}

struct Harness {
    agent: Arc<Agent>,
    storage: Arc<SqliteStorage>,
    spooler: Arc<VirtualSpooler>,
    reporter: Arc<Reporter>,
}

fn office_printer() -> PrinterRef {
    PrinterRef {
        id: PrinterId::new(PRINTER_ID).unwrap(),
        display_name: "Virtual office printer".to_owned(),
        connection: PrinterConnection::Virtual {
            printer_id: "virtual_office".to_owned(),
        },
        submission_mode: SubmissionMode::Driver,
        virtual_profile: Some(VirtualPrinterProfile::SystemDriverPage {
            page_width_mm: 210,
            page_height_mm: 297,
            dpi: 300,
        }),
        enabled: true,
    }
}

fn thermal_printer() -> PrinterRef {
    PrinterRef {
        id: PrinterId::new(PRINTER_ID).unwrap(),
        display_name: "Virtual thermal printer".to_owned(),
        connection: PrinterConnection::Virtual {
            printer_id: "virtual_thermal".to_owned(),
        },
        submission_mode: SubmissionMode::Raw(PrinterLanguage::EscPos),
        virtual_profile: Some(VirtualPrinterProfile::EscPosReceipt { width_mm: 80 }),
        enabled: true,
    }
}

async fn harness(printer: PrinterRef, fetcher: Option<Arc<dyn DocumentFetcher>>) -> Harness {
    let storage = Arc::new(SqliteStorage::in_memory().await.unwrap());
    let spooler = Arc::new(VirtualSpooler::default());
    spooler
        .set_simulation(VirtualSimulation::AlwaysSucceed)
        .await;
    let mut registry = SpoolerRegistry::new();
    registry.register(spooler.clone());
    let resolver = Arc::new(StaticPrinterResolver::new([printer]).unwrap());
    let reporter = Arc::new(Reporter::default());
    let mut builder = AgentBuilder::new()
        .repository(storage.clone())
        .printer_resolver(resolver)
        .spooler_registry(Arc::new(registry))
        .outbound_reporter(reporter.clone());
    if let Some(fetcher) = fetcher {
        builder = builder.document_fetcher(fetcher);
    }
    Harness {
        agent: Arc::new(builder.build().unwrap()),
        storage,
        spooler,
        reporter,
    }
}

fn pdf_source(bytes: &[u8]) -> PdfSource {
    PdfSource::Url {
        url: "https://documents.example.test/v1/documents/job_pdf_1".to_owned(),
        sha256: sha256_hex(bytes),
        byte_length: bytes.len() as u64,
    }
}

fn pdf_job_message(
    job_id: &str,
    source: PdfSource,
    settings: Option<PrintSettings>,
) -> ServerMessage {
    ServerMessage {
        protocol_version: oppa_protocol::ProtocolVersion::CURRENT,
        message_id: format!("msg_{job_id}"),
        sent_at: "2026-07-28T10:00:00Z".to_owned(),
        correlation_id: None,
        kind: ServerMessageKind::PrintJob(PrintJob {
            job_id: job_id.to_owned(),
            idempotency_key: format!("idem_{job_id}"),
            printer_id: PRINTER_ID.to_owned(),
            created_at: "2026-07-28T09:59:00Z".to_owned(),
            document: JobDocument::Pdf(PdfDocument {
                format: PdfFormat::Pdf,
                source,
                settings,
            }),
            metadata: None,
        }),
    }
}

fn failure_code(outcome: &ProcessOutcome) -> String {
    let ProcessOutcome::Failed {
        message: Some(message),
        ..
    } = outcome
    else {
        panic!("expected a reportable failure, got {outcome:?}");
    };
    let AgentMessageKind::JobFailed(payload) = &message.kind else {
        panic!("expected a job_failed message");
    };
    payload.error.code.clone()
}

fn only_outcome(outcomes: &[ProcessOutcome]) -> &ProcessOutcome {
    assert_eq!(outcomes.len(), 1, "exactly one job must be processed");
    &outcomes[0]
}

#[tokio::test]
async fn pdf_job_is_retrieved_verified_rendered_and_submitted() {
    let fetcher: Arc<dyn DocumentFetcher> = Arc::new(ScriptedFetcher {
        behaviour: FetchBehaviour::Serve(PDF.to_vec()),
    });
    let harness = harness(office_printer(), Some(fetcher)).await;
    let message = pdf_job_message(
        "job_pdf_1",
        pdf_source(PDF),
        Some(PrintSettings {
            copies: Some(2),
            page_ranges: Some("1-2".to_owned()),
            ..PrintSettings::default()
        }),
    );

    let result = harness.agent.handle_print_job(&message).await.unwrap();
    assert!(result.receipt.was_inserted());
    let outcome = only_outcome(&result.outcomes);
    assert!(
        matches!(outcome, ProcessOutcome::Submitted { .. }),
        "{outcome:?}"
    );

    let history = harness.spooler.history().await;
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].outcome, VirtualOutcome::Submitted);
    let RenderedDocument::Pdf(document) = &history[0].document else {
        panic!("the spooler must receive the validated PDF");
    };
    assert_eq!(document.selected_pages, vec![0, 1]);
    assert_eq!(document.settings.copies(), 2);
    assert_eq!(history[0].preview.pages.len(), 4, "two pages, two copies");
}

#[tokio::test]
async fn pdf_job_without_a_configured_fetcher_fails_without_submission() {
    let harness = harness(office_printer(), None).await;
    let result = harness
        .agent
        .handle_print_job(&pdf_job_message("job_pdf_2", pdf_source(PDF), None))
        .await
        .unwrap();

    let outcome = only_outcome(&result.outcomes);
    assert_eq!(failure_code(outcome), "document.fetch_unavailable");
    assert!(harness.spooler.history().await.is_empty());
}

#[tokio::test]
async fn integrity_failures_are_permanent_and_never_reach_the_spooler() {
    let fetcher: Arc<dyn DocumentFetcher> = Arc::new(ScriptedFetcher {
        behaviour: FetchBehaviour::Fail(|| DocumentFetchError::IntegrityMismatch),
    });
    let harness = harness(office_printer(), Some(fetcher)).await;
    let result = harness
        .agent
        .handle_print_job(&pdf_job_message("job_pdf_3", pdf_source(PDF), None))
        .await
        .unwrap();

    let outcome = only_outcome(&result.outcomes);
    assert_eq!(failure_code(outcome), "document.integrity_failed");
    assert!(harness.spooler.history().await.is_empty());
    assert!(
        harness
            .storage
            .has_completed_idempotency_key("idem_job_pdf_3")
            .await
            .unwrap(),
        "the failure must be committed durably before it is reported"
    );
}

#[tokio::test]
async fn transient_retrieval_failures_are_marked_retryable() {
    let fetcher: Arc<dyn DocumentFetcher> = Arc::new(ScriptedFetcher {
        behaviour: FetchBehaviour::Fail(|| DocumentFetchError::Transport("reset".to_owned())),
    });
    let harness = harness(office_printer(), Some(fetcher)).await;
    let result = harness
        .agent
        .handle_print_job(&pdf_job_message("job_pdf_4", pdf_source(PDF), None))
        .await
        .unwrap();

    let outcome = only_outcome(&result.outcomes);
    let ProcessOutcome::Failed {
        message: Some(message),
        ..
    } = outcome
    else {
        panic!("expected a reportable failure");
    };
    let AgentMessageKind::JobFailed(payload) = &message.kind else {
        panic!("expected a job_failed message");
    };
    assert_eq!(payload.error.code, "document.fetch_failed");
    assert!(payload.error.retryable);
}

#[tokio::test]
async fn invalid_pdf_bytes_fail_validation_before_submission() {
    let garbage = b"%PDF-1.4 this is not a complete document".to_vec();
    let fetcher: Arc<dyn DocumentFetcher> = Arc::new(ScriptedFetcher {
        behaviour: FetchBehaviour::Serve(garbage.clone()),
    });
    let harness = harness(office_printer(), Some(fetcher)).await;
    let result = harness
        .agent
        .handle_print_job(&pdf_job_message("job_pdf_5", pdf_source(&garbage), None))
        .await
        .unwrap();

    let outcome = only_outcome(&result.outcomes);
    assert_eq!(failure_code(outcome), "document.invalid");
    assert!(harness.spooler.history().await.is_empty());
}

#[tokio::test]
async fn receipt_only_printers_reject_pdf_jobs_with_an_actionable_error() {
    let fetcher: Arc<dyn DocumentFetcher> = Arc::new(ScriptedFetcher {
        behaviour: FetchBehaviour::Serve(PDF.to_vec()),
    });
    let harness = harness(thermal_printer(), Some(fetcher)).await;
    let result = harness
        .agent
        .handle_print_job(&pdf_job_message("job_pdf_6", pdf_source(PDF), None))
        .await
        .unwrap();

    let outcome = only_outcome(&result.outcomes);
    assert_eq!(failure_code(outcome), "document.format_unsupported");
    assert!(harness.spooler.history().await.is_empty());
}

#[tokio::test]
async fn duplex_requests_are_passed_through_to_the_spooler_contract() {
    let fetcher: Arc<dyn DocumentFetcher> = Arc::new(ScriptedFetcher {
        behaviour: FetchBehaviour::Serve(PDF.to_vec()),
    });
    let harness = harness(office_printer(), Some(fetcher)).await;
    let result = harness
        .agent
        .handle_print_job(&pdf_job_message(
            "job_pdf_7",
            pdf_source(PDF),
            Some(PrintSettings {
                duplex: Some(Duplex::LongEdge),
                ..PrintSettings::default()
            }),
        ))
        .await
        .unwrap();

    assert!(matches!(
        only_outcome(&result.outcomes),
        ProcessOutcome::Submitted { .. }
    ));
    let history = harness.spooler.history().await;
    let RenderedDocument::Pdf(document) = &history[0].document else {
        panic!("expected a PDF document");
    };
    assert_eq!(document.settings.duplex(), Duplex::LongEdge);
}

#[tokio::test]
async fn local_pdf_tests_are_submitted_without_durable_state_or_reports() {
    let harness = harness(office_printer(), None).await;
    let receipt = harness
        .agent
        .submit_local_pdf(
            &PrintJobId::new("local_pdf_1").unwrap(),
            &PrinterId::new(PRINTER_ID).unwrap(),
            PDF.to_vec(),
            &PrintSettings::default(),
        )
        .await
        .expect("local PDF test must submit");

    assert_eq!(receipt.backend, "virtual");
    assert!(harness.storage.pending().await.unwrap().is_empty());
    assert!(harness.reporter.messages.lock().await.is_empty());
    assert_eq!(harness.spooler.history().await.len(), 1);
}

#[tokio::test]
async fn local_pdf_tests_reject_non_pdf_content_before_submission() {
    let harness = harness(office_printer(), None).await;
    let error = harness
        .agent
        .submit_local_pdf(
            &PrintJobId::new("local_pdf_2").unwrap(),
            &PrinterId::new(PRINTER_ID).unwrap(),
            b"plain text".to_vec(),
            &PrintSettings::default(),
        )
        .await
        .expect_err("plain text is not a PDF");

    assert_eq!(error.code(), "document.invalid");
    assert!(harness.spooler.history().await.is_empty());
}
