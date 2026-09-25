//! Concrete printer submission transports with explicit deadlines.
//!
//! A successful [`SubmissionReceipt`] means a backend accepted all bytes. It
//! does not claim that paper was physically produced.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    sync::Arc,
    time::Duration,
};

#[cfg(unix)]
use std::{ffi::OsString, path::PathBuf, process::Stdio};

use async_trait::async_trait;
use oppa_core::{PrintJobId, PrinterId, Timestamp};
use oppa_printer::{
    ConnectionKind, PrinterConnection, PrinterLanguage, PrinterRef, SubmissionMode,
    SubmissionReceipt, VirtualPrinterProfile,
};
#[cfg(unix)]
use oppa_renderer::encode_raster_page_png;
use oppa_renderer::{
    EscPosDiagnostics, EscPosInterpreter, PageRenderOptions, RasterDocument, RasterPage,
    ReceiptWidth, RenderedDocument, render_escpos_for_page, render_layout_receipt,
};
use thiserror::Error;
use tokio::{
    io::AsyncWriteExt,
    net::TcpStream,
    sync::Mutex,
    time::{sleep, timeout},
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Default upper bound for one spooler submission payload.
pub const DEFAULT_MAX_SUBMISSION_BYTES: usize = 4 * 1024 * 1024;
const DEFAULT_MAX_VIRTUAL_HISTORY: usize = 20;

/// Borrowed submission inputs passed to a concrete spooler.
pub struct SubmissionRequest<'a> {
    /// Durable job identity.
    pub job_id: &'a PrintJobId,
    /// Configured and enabled local printer.
    pub printer: &'a PrinterRef,
    /// Previously rendered output.
    pub document: &'a RenderedDocument,
}

/// Printer submission boundary implemented by concrete backends.
#[async_trait]
pub trait Spooler: Send + Sync {
    /// Returns the one connection family handled by this implementation.
    fn connection_kind(&self) -> ConnectionKind;

    /// Submits one rendered document with cooperative cancellation.
    async fn submit(
        &self,
        request: SubmissionRequest<'_>,
        cancellation: &CancellationToken,
    ) -> SpoolerResult<SubmissionReceipt>;
}

/// Routes configured printers to their concrete spooler.
#[derive(Default)]
pub struct SpoolerRegistry {
    spoolers: HashMap<ConnectionKind, Arc<dyn Spooler>>,
}

impl SpoolerRegistry {
    /// Creates an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds or replaces the implementation for its connection family.
    pub fn register(&mut self, spooler: Arc<dyn Spooler>) {
        self.spoolers.insert(spooler.connection_kind(), spooler);
    }

    /// Validates the target and routes a submission.
    pub async fn submit(
        &self,
        request: SubmissionRequest<'_>,
        cancellation: &CancellationToken,
    ) -> SpoolerResult<SubmissionReceipt> {
        request
            .printer
            .validate()
            .map_err(|error| SpoolerError::InvalidTarget(error.to_string()))?;
        if !request.printer.enabled {
            return Err(SpoolerError::PrinterDisabled(request.printer.id.clone()));
        }
        let kind = request.printer.connection.kind();
        let spooler = self
            .spoolers
            .get(&kind)
            .ok_or(SpoolerError::UnsupportedTarget(kind))?;
        spooler.submit(request, cancellation).await
    }
}

/// Raw TCP printer submission, commonly used with port 9100.
#[derive(Debug, Clone, Copy)]
pub struct RawTcpSpooler {
    /// Maximum time allowed to resolve and establish TCP.
    pub connect_timeout: Duration,
    /// Maximum time allowed to write and close the stream.
    pub write_timeout: Duration,
    /// Maximum document bytes accepted.
    pub max_submission_bytes: usize,
}

impl Default for RawTcpSpooler {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(5),
            write_timeout: Duration::from_secs(15),
            max_submission_bytes: DEFAULT_MAX_SUBMISSION_BYTES,
        }
    }
}

#[async_trait]
impl Spooler for RawTcpSpooler {
    fn connection_kind(&self) -> ConnectionKind {
        ConnectionKind::Network
    }

    async fn submit(
        &self,
        request: SubmissionRequest<'_>,
        cancellation: &CancellationToken,
    ) -> SpoolerResult<SubmissionReceipt> {
        let PrinterConnection::Network { host, port } = &request.printer.connection else {
            return Err(SpoolerError::UnsupportedTarget(
                request.printer.connection.kind(),
            ));
        };
        require_raw_mode(request.printer)?;
        let bytes = raw_bytes(request.document)?;
        enforce_size(bytes.len(), self.max_submission_bytes)?;

        let connect = timeout(
            self.connect_timeout,
            TcpStream::connect((host.as_str(), *port)),
        );
        let mut stream = tokio::select! {
            () = cancellation.cancelled() => return Err(SpoolerError::Cancelled),
            result = connect => {
                result
                    .map_err(|_| SpoolerError::Timeout {
                        stage: "connect",
                        duration: self.connect_timeout,
                    })?
                    .map_err(|error| SpoolerError::Connectivity(error.to_string()))?
            }
        };

        let write = async {
            stream.write_all(bytes).await?;
            stream.flush().await?;
            stream.shutdown().await
        };
        tokio::select! {
            () = cancellation.cancelled() => return Err(SpoolerError::Cancelled),
            result = timeout(self.write_timeout, write) => {
                result
                    .map_err(|_| SpoolerError::Timeout {
                        stage: "write",
                        duration: self.write_timeout,
                    })?
                    .map_err(|error| SpoolerError::Connectivity(error.to_string()))?;
            }
        }

        Ok(receipt(
            "raw-tcp",
            None,
            BTreeMap::from([
                ("endpoint".to_owned(), format!("{host}:{port}")),
                ("submissionMode".to_owned(), "raw".to_owned()),
                ("language".to_owned(), "esc-pos".to_owned()),
            ]),
        ))
    }
}

/// Operating-system queue spooler.
#[derive(Debug, Clone, Copy)]
pub struct SystemQueueSpooler {
    /// Maximum time allowed for the platform queue command.
    pub submission_timeout: Duration,
    /// Maximum document bytes accepted.
    pub max_submission_bytes: usize,
}

impl Default for SystemQueueSpooler {
    fn default() -> Self {
        Self {
            submission_timeout: Duration::from_secs(30),
            max_submission_bytes: DEFAULT_MAX_SUBMISSION_BYTES,
        }
    }
}

#[async_trait]
impl Spooler for SystemQueueSpooler {
    fn connection_kind(&self) -> ConnectionKind {
        ConnectionKind::SystemQueue
    }

    async fn submit(
        &self,
        request: SubmissionRequest<'_>,
        cancellation: &CancellationToken,
    ) -> SpoolerResult<SubmissionReceipt> {
        let PrinterConnection::SystemQueue { queue_name } = &request.printer.connection else {
            return Err(SpoolerError::UnsupportedTarget(
                request.printer.connection.kind(),
            ));
        };
        let prepared = prepare_system_queue_submission(
            request.printer,
            request.document,
            self.max_submission_bytes,
        )?;
        submit_system_queue(queue_name, prepared, self.submission_timeout, cancellation).await
    }
}

enum SystemQueuePayload {
    RawEscPos(Vec<u8>),
    DriverPages {
        document: RasterDocument,
        compatibility: Option<EscPosDiagnostics>,
    },
}

fn prepare_system_queue_submission(
    printer: &PrinterRef,
    document: &RenderedDocument,
    max_submission_bytes: usize,
) -> SpoolerResult<SystemQueuePayload> {
    let payload = match printer.submission_mode {
        SubmissionMode::Raw(PrinterLanguage::EscPos) => {
            SystemQueuePayload::RawEscPos(raw_bytes(document)?.to_vec())
        }
        SubmissionMode::Driver => {
            let (document, compatibility) = driver_pages(document)?;
            SystemQueuePayload::DriverPages {
                document,
                compatibility,
            }
        }
    };
    let byte_len = match &payload {
        SystemQueuePayload::RawEscPos(bytes) => bytes.len(),
        SystemQueuePayload::DriverPages { document, .. } => {
            document.pages.iter().map(|page| page.data.len()).sum()
        }
    };
    enforce_size(byte_len, max_submission_bytes)?;
    Ok(payload)
}

fn driver_pages(
    document: &RenderedDocument,
) -> SpoolerResult<(RasterDocument, Option<EscPosDiagnostics>)> {
    match document {
        RenderedDocument::Raster(document) => Ok((document.clone(), None)),
        RenderedDocument::EscPos(document) => {
            let (page, diagnostics) = render_escpos_for_page(
                &document.bytes,
                document.receipt_width,
                PageRenderOptions::A4_PORTRAIT,
            )
            .map_err(|error| SpoolerError::DocumentRender(error.to_string()))?;
            Ok((page, Some(diagnostics)))
        }
        other => Err(SpoolerError::UnsupportedDocument {
            backend: "system driver",
            document: other.kind(),
        }),
    }
}

fn require_raw_mode(printer: &PrinterRef) -> SpoolerResult<()> {
    if matches!(
        printer.submission_mode,
        SubmissionMode::Raw(PrinterLanguage::EscPos)
    ) {
        Ok(())
    } else {
        Err(SpoolerError::InvalidSubmissionMode(
            "raw TCP requires explicit raw ESC/POS mode".to_owned(),
        ))
    }
}

#[cfg(unix)]
async fn submit_system_queue(
    queue_name: &str,
    payload: SystemQueuePayload,
    deadline: Duration,
    cancellation: &CancellationToken,
) -> SpoolerResult<SubmissionReceipt> {
    match payload {
        SystemQueuePayload::RawEscPos(bytes) => {
            submit_cups_raw(queue_name, &bytes, deadline, cancellation).await
        }
        SystemQueuePayload::DriverPages {
            document,
            compatibility,
        } => submit_cups_driver(queue_name, &document, compatibility, deadline, cancellation).await,
    }
}

#[cfg(windows)]
async fn submit_system_queue(
    queue_name: &str,
    payload: SystemQueuePayload,
    deadline: Duration,
    cancellation: &CancellationToken,
) -> SpoolerResult<SubmissionReceipt> {
    let queue_name = queue_name.to_owned();
    let worker = tokio::task::spawn_blocking(move || match payload {
        SystemQueuePayload::RawEscPos(bytes) => windows_print_raw(&queue_name, &bytes),
        SystemQueuePayload::DriverPages {
            document,
            compatibility,
        } => windows_print_driver(&queue_name, &document, compatibility),
    });
    tokio::select! {
        () = cancellation.cancelled() => Err(SpoolerError::Cancelled),
        result = timeout(deadline, worker) => {
            result
                .map_err(|_| SpoolerError::Timeout {
                    stage: "system queue submission",
                    duration: deadline,
                })?
                .map_err(|error| SpoolerError::BackendUnavailable(format!(
                    "print worker did not finish: {error}"
                )))?
        }
    }
}

#[cfg(unix)]
async fn submit_cups_raw(
    queue_name: &str,
    bytes: &[u8],
    deadline: Duration,
    cancellation: &CancellationToken,
) -> SpoolerResult<SubmissionReceipt> {
    let mut child = tokio::process::Command::new("lp")
        .args(cups_raw_args(queue_name))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| SpoolerError::BackendUnavailable(format!("cannot start lp: {error}")))?;
    let mut stdin = child.stdin.take().ok_or_else(|| {
        SpoolerError::BackendUnavailable("lp did not expose a standard input pipe".to_owned())
    })?;
    let operation = async {
        stdin
            .write_all(bytes)
            .await
            .map_err(|error| SpoolerError::Connectivity(error.to_string()))?;
        stdin
            .shutdown()
            .await
            .map_err(|error| SpoolerError::Connectivity(error.to_string()))?;
        drop(stdin);
        let output = child
            .wait_with_output()
            .await
            .map_err(|error| SpoolerError::Connectivity(error.to_string()))?;
        lp_receipt(queue_name, output, "cups-raw", "raw", Some("esc-pos"), None)
    };
    tokio::select! {
        () = cancellation.cancelled() => Err(SpoolerError::Cancelled),
        result = timeout(deadline, operation) => {
            result.map_err(|_| SpoolerError::Timeout {
                stage: "system queue submission",
                duration: deadline,
            })?
        }
    }
}

#[cfg(unix)]
async fn submit_cups_driver(
    queue_name: &str,
    document: &RasterDocument,
    compatibility: Option<EscPosDiagnostics>,
    deadline: Duration,
    cancellation: &CancellationToken,
) -> SpoolerResult<SubmissionReceipt> {
    if document.pages.is_empty() {
        return Err(SpoolerError::DocumentRender(
            "driver document contains no pages".to_owned(),
        ));
    }
    let mut files = Vec::with_capacity(document.pages.len());
    for page in &document.pages {
        let image = encode_raster_page_png(page)
            .map_err(|error| SpoolerError::DocumentRender(error.to_string()))?;
        let file = tempfile::Builder::new()
            .prefix("oppa-print-")
            .suffix(".png")
            .tempfile()
            .map_err(|error| SpoolerError::BackendUnavailable(error.to_string()))?;
        std::fs::write(file.path(), image)
            .map_err(|error| SpoolerError::Connectivity(error.to_string()))?;
        files.push(file);
    }
    let paths = files
        .iter()
        .map(|file| file.path().to_owned())
        .collect::<Vec<_>>();
    let mut command = tokio::process::Command::new("lp");
    command
        .args(cups_driver_args(queue_name, &paths))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let operation = async {
        let output = command.output().await.map_err(|error| {
            SpoolerError::BackendUnavailable(format!("cannot start lp: {error}"))
        })?;
        lp_receipt(
            queue_name,
            output,
            "cups-filter",
            "driver",
            None,
            compatibility,
        )
    };
    tokio::select! {
        () = cancellation.cancelled() => Err(SpoolerError::Cancelled),
        result = timeout(deadline, operation) => {
            result.map_err(|_| SpoolerError::Timeout {
                stage: "system queue submission",
                duration: deadline,
            })?
        }
    }
}

#[cfg(unix)]
fn cups_raw_args(queue_name: &str) -> Vec<OsString> {
    ["-d", queue_name, "-o", "raw"]
        .into_iter()
        .map(OsString::from)
        .collect()
}

#[cfg(unix)]
fn cups_driver_args(queue_name: &str, paths: &[PathBuf]) -> Vec<OsString> {
    let mut args = vec![OsString::from("-d"), OsString::from(queue_name)];
    args.extend(paths.iter().map(|path| path.as_os_str().to_owned()));
    args
}

#[cfg(unix)]
fn lp_receipt(
    queue_name: &str,
    output: std::process::Output,
    backend: &str,
    mode: &str,
    language: Option<&str>,
    compatibility: Option<EscPosDiagnostics>,
) -> SpoolerResult<SubmissionReceipt> {
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SpoolerError::Rejected(
            stderr.trim().chars().take(1_000).collect(),
        ));
    }
    let mut metadata = BTreeMap::from([
        ("queue".to_owned(), queue_name.to_owned()),
        ("submissionMode".to_owned(), mode.to_owned()),
    ]);
    if let Some(language) = language {
        metadata.insert("language".to_owned(), language.to_owned());
    }
    if let Some(diagnostics) = compatibility {
        metadata.insert(
            "compatibilityCommands".to_owned(),
            diagnostics.interpreted_commands.to_string(),
        );
        metadata.insert(
            "compatibilityCutRequested".to_owned(),
            diagnostics.cut_requested.to_string(),
        );
    }
    Ok(receipt(
        backend,
        parse_lp_job_id(&String::from_utf8_lossy(&output.stdout)),
        metadata,
    ))
}

/// Sends raw bytes to a Windows print queue as a `RAW` datatype job through the
/// Win32 spooler (`OpenPrinter`/`StartDocPrinter`/`WritePrinter`).
///
/// This is a blocking call and must run on a blocking-safe task.
#[cfg(windows)]
fn windows_print_raw(queue_name: &str, bytes: &[u8]) -> SpoolerResult<SubmissionReceipt> {
    use printers::common::base::job::PrinterJobOptions;

    let printer = printers::get_printer_by_name(queue_name).ok_or_else(|| {
        SpoolerError::Connectivity(format!(
            "the Windows print spooler has no queue named {queue_name}"
        ))
    })?;
    let backend_job_id = printer
        .print(bytes, PrinterJobOptions::none())
        .map_err(|error| {
            SpoolerError::Rejected(error.message.chars().take(1_000).collect::<String>())
        })?;
    Ok(receipt(
        "windows-spooler",
        Some(backend_job_id.to_string()),
        BTreeMap::from([
            ("queue".to_owned(), queue_name.to_owned()),
            ("submissionMode".to_owned(), "raw".to_owned()),
            ("language".to_owned(), "esc-pos".to_owned()),
        ]),
    ))
}

#[cfg(windows)]
fn windows_print_driver(
    queue_name: &str,
    document: &RasterDocument,
    compatibility: Option<EscPosDiagnostics>,
) -> SpoolerResult<SubmissionReceipt> {
    let backend_job_id = oppa_windows_print::print_document(queue_name, document)
        .map_err(|error| SpoolerError::Rejected(error.to_string()))?;
    let mut metadata = BTreeMap::from([
        ("queue".to_owned(), queue_name.to_owned()),
        ("submissionMode".to_owned(), "driver".to_owned()),
        ("pageCount".to_owned(), document.pages.len().to_string()),
    ]);
    if let Some(diagnostics) = compatibility {
        metadata.insert(
            "compatibilityCommands".to_owned(),
            diagnostics.interpreted_commands.to_string(),
        );
        metadata.insert(
            "compatibilityCutRequested".to_owned(),
            diagnostics.cut_requested.to_string(),
        );
    }
    Ok(receipt(
        "windows-gdi",
        Some(backend_job_id.to_string()),
        metadata,
    ))
}

#[cfg(not(any(unix, windows)))]
async fn submit_system_queue(
    _queue_name: &str,
    _payload: SystemQueuePayload,
    _deadline: Duration,
    _cancellation: &CancellationToken,
) -> SpoolerResult<SubmissionReceipt> {
    Err(SpoolerError::BackendUnavailable(
        "system queue submission is not implemented on this platform".to_owned(),
    ))
}

#[cfg(unix)]
fn parse_lp_job_id(output: &str) -> Option<String> {
    // Common CUPS output: "request id is queue-123 (1 file(s))".
    output
        .split_whitespace()
        .skip_while(|word| *word != "is")
        .nth(1)
        .filter(|value| !value.is_empty() && value.len() <= 256)
        .map(str::to_owned)
}

fn raw_bytes(document: &RenderedDocument) -> SpoolerResult<&[u8]> {
    match document {
        RenderedDocument::EscPos(document) => Ok(&document.bytes),
        other => Err(SpoolerError::UnsupportedDocument {
            backend: "raw byte spooler",
            document: other.kind(),
        }),
    }
}

fn enforce_size(actual: usize, maximum: usize) -> SpoolerResult<()> {
    if maximum == 0 || actual > maximum {
        Err(SpoolerError::DocumentTooLarge { actual, maximum })
    } else {
        Ok(())
    }
}

/// Configurable virtual-printer behavior.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VirtualSimulation {
    /// Accept every submission immediately.
    #[default]
    AlwaysSucceed,
    /// Reject one submission, then return to `AlwaysSucceed`.
    FailNext,
    /// Reject every submission.
    AlwaysFail,
    /// Wait for the supplied duration, then accept.
    Delay(Duration),
    /// Behave as an unavailable printer.
    Offline,
}

/// Recorded outcome of a virtual-printer attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtualOutcome {
    /// The simulated backend accepted the document.
    Submitted,
    /// The simulator intentionally failed the document.
    Failed,
    /// The simulated printer was offline.
    Offline,
}

/// Bounded virtual-printer history entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualSubmission {
    /// Durable job identity.
    pub job_id: PrintJobId,
    /// Stable virtual printer identity.
    pub printer_id: PrinterId,
    /// Time the simulated outcome was recorded.
    pub recorded_at: Timestamp,
    /// Simulated outcome.
    pub outcome: VirtualOutcome,
    /// Complete rendered output for the local inspector.
    pub document: RenderedDocument,
    /// Result after the concrete emulated device interpreted its input.
    pub preview: VirtualPreview,
}

/// Interpreted output produced by one virtual printer profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualPreview {
    /// Inspectable receipt or office page raster.
    pub pages: Vec<RasterPage>,
    /// ESC/POS interpreter diagnostics when the input passed through it.
    pub escpos: Option<EscPosDiagnostics>,
}

#[derive(Default)]
struct VirtualState {
    simulation: VirtualSimulation,
    history: VecDeque<VirtualSubmission>,
}

/// In-process spooler used for development, tests, and receipt inspection.
#[derive(Clone)]
pub struct VirtualSpooler {
    state: Arc<Mutex<VirtualState>>,
    max_history: usize,
    max_submission_bytes: usize,
}

impl VirtualSpooler {
    /// Creates a bounded virtual spooler.
    pub fn new(max_history: usize, max_submission_bytes: usize) -> SpoolerResult<Self> {
        if max_history == 0 || max_submission_bytes == 0 {
            return Err(SpoolerError::InvalidConfiguration(
                "virtual history and submission limits must be greater than zero".to_owned(),
            ));
        }
        Ok(Self {
            state: Arc::new(Mutex::new(VirtualState::default())),
            max_history,
            max_submission_bytes,
        })
    }

    /// Changes future simulated behavior.
    pub async fn set_simulation(&self, simulation: VirtualSimulation) {
        self.state.lock().await.simulation = simulation;
    }

    /// Returns history from oldest to newest.
    pub async fn history(&self) -> Vec<VirtualSubmission> {
        self.state.lock().await.history.iter().cloned().collect()
    }

    /// Clears retained output without altering simulation behavior.
    pub async fn clear_history(&self) {
        self.state.lock().await.history.clear();
    }

    async fn record(&self, submission: VirtualSubmission) {
        let mut state = self.state.lock().await;
        while state.history.len() >= self.max_history {
            state.history.pop_front();
        }
        state.history.push_back(submission);
    }
}

impl Default for VirtualSpooler {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(VirtualState::default())),
            max_history: DEFAULT_MAX_VIRTUAL_HISTORY,
            max_submission_bytes: DEFAULT_MAX_SUBMISSION_BYTES,
        }
    }
}

#[async_trait]
impl Spooler for VirtualSpooler {
    fn connection_kind(&self) -> ConnectionKind {
        ConnectionKind::Virtual
    }

    async fn submit(
        &self,
        request: SubmissionRequest<'_>,
        cancellation: &CancellationToken,
    ) -> SpoolerResult<SubmissionReceipt> {
        enforce_size(request.document.byte_len(), self.max_submission_bytes)?;
        let preview = virtual_preview(request.printer, request.document)?;
        let simulation = {
            let mut state = self.state.lock().await;
            let current = state.simulation;
            if current == VirtualSimulation::FailNext {
                state.simulation = VirtualSimulation::AlwaysSucceed;
            }
            current
        };

        if let VirtualSimulation::Delay(duration) = simulation {
            tokio::select! {
                () = cancellation.cancelled() => return Err(SpoolerError::Cancelled),
                () = sleep(duration) => {}
            }
        }
        if cancellation.is_cancelled() {
            return Err(SpoolerError::Cancelled);
        }

        let outcome = match simulation {
            VirtualSimulation::AlwaysSucceed | VirtualSimulation::Delay(_) => {
                VirtualOutcome::Submitted
            }
            VirtualSimulation::FailNext | VirtualSimulation::AlwaysFail => VirtualOutcome::Failed,
            VirtualSimulation::Offline => VirtualOutcome::Offline,
        };
        self.record(VirtualSubmission {
            job_id: request.job_id.clone(),
            printer_id: request.printer.id.clone(),
            recorded_at: Timestamp::now(),
            outcome,
            document: request.document.clone(),
            preview: preview.clone(),
        })
        .await;

        match outcome {
            VirtualOutcome::Submitted => {
                let backend_job_id = Uuid::new_v4().to_string();
                let mut metadata = BTreeMap::from([
                    (
                        "submissionMode".to_owned(),
                        match request.printer.submission_mode {
                            SubmissionMode::Driver => "driver",
                            SubmissionMode::Raw(PrinterLanguage::EscPos) => "raw",
                        }
                        .to_owned(),
                    ),
                    ("pageCount".to_owned(), preview.pages.len().to_string()),
                ]);
                if let Some(diagnostics) = preview.escpos {
                    metadata.insert(
                        "interpretedCommands".to_owned(),
                        diagnostics.interpreted_commands.to_string(),
                    );
                    metadata.insert(
                        "cutRequested".to_owned(),
                        diagnostics.cut_requested.to_string(),
                    );
                }
                Ok(receipt("virtual", Some(backend_job_id), metadata))
            }
            VirtualOutcome::Failed => Err(SpoolerError::SimulatedFailure),
            VirtualOutcome::Offline => Err(SpoolerError::Connectivity(
                "virtual printer is offline".to_owned(),
            )),
        }
    }
}

fn virtual_preview(
    printer: &PrinterRef,
    document: &RenderedDocument,
) -> SpoolerResult<VirtualPreview> {
    let profile = printer.virtual_profile.ok_or_else(|| {
        SpoolerError::InvalidTarget("virtual printer profile is missing".to_owned())
    })?;
    match profile {
        VirtualPrinterProfile::EscPosReceipt { width_mm } => {
            let RenderedDocument::EscPos(document) = document else {
                return Err(SpoolerError::UnsupportedDocument {
                    backend: "virtual ESC/POS device",
                    document: document.kind(),
                });
            };
            let width = receipt_width(width_mm)?;
            let interpretation = EscPosInterpreter::new(width)
                .interpret(&document.bytes)
                .map_err(|error| SpoolerError::DocumentRender(error.to_string()))?;
            let preview = render_layout_receipt(&interpretation.layout, 203)
                .map_err(|error| SpoolerError::DocumentRender(error.to_string()))?;
            Ok(VirtualPreview {
                pages: preview.pages,
                escpos: Some(interpretation.diagnostics),
            })
        }
        VirtualPrinterProfile::SystemDriverPage {
            page_width_mm,
            page_height_mm,
            dpi,
        } => {
            let options = PageRenderOptions {
                width_mm: page_width_mm,
                height_mm: page_height_mm,
                dpi,
            };
            match document {
                RenderedDocument::Raster(document) => Ok(VirtualPreview {
                    pages: document.pages.clone(),
                    escpos: None,
                }),
                RenderedDocument::EscPos(document) => {
                    let (page, diagnostics) =
                        render_escpos_for_page(&document.bytes, document.receipt_width, options)
                            .map_err(|error| SpoolerError::DocumentRender(error.to_string()))?;
                    Ok(VirtualPreview {
                        pages: page.pages,
                        escpos: Some(diagnostics),
                    })
                }
                other => Err(SpoolerError::UnsupportedDocument {
                    backend: "virtual system driver device",
                    document: other.kind(),
                }),
            }
        }
    }
}

fn receipt_width(width_mm: u16) -> SpoolerResult<ReceiptWidth> {
    match width_mm {
        58 => Ok(ReceiptWidth::Mm58),
        80 => Ok(ReceiptWidth::Mm80),
        _ => Err(SpoolerError::InvalidTarget(
            "virtual receipt width must be 58 or 80 millimetres".to_owned(),
        )),
    }
}

fn receipt(
    backend: &str,
    backend_job_id: Option<String>,
    metadata: BTreeMap<String, String>,
) -> SubmissionReceipt {
    SubmissionReceipt {
        backend_job_id,
        backend: backend.to_owned(),
        accepted_at: Timestamp::now(),
        metadata,
    }
}

/// Structured spooler failures.
#[derive(Debug, Error)]
pub enum SpoolerError {
    /// Printer reference was invalid.
    #[error("printer target is invalid: {0}")]
    InvalidTarget(String),
    /// The configured printer was disabled.
    #[error("printer {0} is disabled")]
    PrinterDisabled(PrinterId),
    /// No spooler was registered for the connection family.
    #[error("no spooler supports target type {0:?}")]
    UnsupportedTarget(ConnectionKind),
    /// The renderer output family is not accepted by a backend.
    #[error("{backend} does not support {document} documents")]
    UnsupportedDocument {
        /// Backend family.
        backend: &'static str,
        /// Rendered document family.
        document: &'static str,
    },
    /// The configured submission mode cannot reach the selected transport.
    #[error("invalid printer submission mode: {0}")]
    InvalidSubmissionMode(String),
    /// A rendered document could not be converted to the selected backend format.
    #[error("document rendering failed: {0}")]
    DocumentRender(String),
    /// Input exceeded the backend's bounded payload.
    #[error("document is {actual} bytes; spooler maximum is {maximum}")]
    DocumentTooLarge {
        /// Actual bytes.
        actual: usize,
        /// Configured bound.
        maximum: usize,
    },
    /// Backend executable or platform integration was absent.
    #[error("spooler backend is unavailable: {0}")]
    BackendUnavailable(String),
    /// The target disappeared or could not be reached.
    #[error("printer connectivity failed: {0}")]
    Connectivity(String),
    /// A backend exceeded an explicit stage deadline.
    #[error("printer {stage} timed out after {duration:?}")]
    Timeout {
        /// Operation stage.
        stage: &'static str,
        /// Configured deadline.
        duration: Duration,
    },
    /// The operating-system queue rejected input.
    #[error("printer backend rejected submission: {0}")]
    Rejected(String),
    /// Cooperative cancellation won before backend acceptance.
    #[error("printer submission was cancelled")]
    Cancelled,
    /// Virtual printer intentionally simulated failure.
    #[error("virtual printer simulated a submission failure")]
    SimulatedFailure,
    /// Spooler limits were invalid.
    #[error("invalid spooler configuration: {0}")]
    InvalidConfiguration(String),
}

/// Result alias for spooler operations.
pub type SpoolerResult<T> = Result<T, SpoolerError>;

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use image::ImageEncoder;
    use oppa_printer::VirtualPrinterProfile;
    use oppa_renderer::{
        DocumentRenderer, EscPosDocument, PageRenderOptions, RenderTarget, RenderedDocument,
        VirtualPrintDocument,
    };

    use super::*;

    fn job_id() -> PrintJobId {
        PrintJobId::new("job_1").expect("fixture job")
    }

    fn virtual_printer() -> PrinterRef {
        PrinterRef {
            id: PrinterId::new("virtual_1").expect("fixture printer"),
            display_name: "Virtual receipt".to_owned(),
            connection: PrinterConnection::Virtual {
                printer_id: "virtual_1".to_owned(),
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

    fn virtual_document() -> RenderedDocument {
        DocumentRenderer::default()
            .render(
                &oppa_protocol_fixture(),
                RenderTarget::Page(PageRenderOptions::A4_PORTRAIT),
            )
            .expect("office page")
    }

    fn oppa_protocol_fixture() -> oppa_protocol::PrintDocument {
        let image = image::GrayImage::from_fn(8, 8, |x, y| {
            if x == y || x + y == 7 {
                image::Luma([0])
            } else {
                image::Luma([255])
            }
        });
        let mut png = Cursor::new(Vec::new());
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(
                image.as_raw(),
                image.width(),
                image.height(),
                image::ExtendedColorType::L8,
            )
            .expect("encode fixture image");
        oppa_protocol::PrintDocument {
            width: oppa_protocol::ReceiptWidth::Mm58,
            sections: vec![
                oppa_protocol::PrintSection::Text {
                    value: "Centered title".to_owned(),
                    align: Some(oppa_protocol::TextAlignment::Center),
                    bold: Some(true),
                },
                oppa_protocol::PrintSection::Text {
                    value: "Normal text".to_owned(),
                    align: None,
                    bold: None,
                },
                oppa_protocol::PrintSection::Row {
                    left: "Coffee".to_owned(),
                    right: "120.00".to_owned(),
                },
                oppa_protocol::PrintSection::Divider,
                oppa_protocol::PrintSection::Image {
                    media_type: oppa_protocol::ImageMediaType::Png,
                    data: STANDARD.encode(png.into_inner()),
                },
                oppa_protocol::PrintSection::Qr {
                    value: "https://openprinter.dev/test".to_owned(),
                },
                oppa_protocol::PrintSection::Barcode {
                    format: oppa_protocol::BarcodeFormat::Code128,
                    value: "RECEIPT-123".to_owned(),
                },
                oppa_protocol::PrintSection::Feed { lines: 2 },
                oppa_protocol::PrintSection::Cut,
            ],
        }
    }

    fn thermal_printer(width_mm: u16) -> PrinterRef {
        PrinterRef {
            id: PrinterId::new("virtual_thermal").expect("fixture printer"),
            display_name: "Virtual thermal".to_owned(),
            connection: PrinterConnection::Virtual {
                printer_id: "virtual_thermal".to_owned(),
            },
            submission_mode: SubmissionMode::Raw(PrinterLanguage::EscPos),
            virtual_profile: Some(VirtualPrinterProfile::EscPosReceipt { width_mm }),
            enabled: true,
        }
    }

    fn legacy_virtual_document() -> RenderedDocument {
        RenderedDocument::Virtual(VirtualPrintDocument {
            document: oppa_protocol_fixture(),
            preview_lines: vec!["Test".to_owned()],
        })
    }

    fn system_queue(mode: SubmissionMode) -> PrinterRef {
        PrinterRef {
            id: PrinterId::new("system_1").expect("fixture printer"),
            display_name: "System queue".to_owned(),
            connection: PrinterConnection::SystemQueue {
                queue_name: "Office".to_owned(),
            },
            submission_mode: mode,
            virtual_profile: None,
            enabled: true,
        }
    }

    fn escpos_document() -> RenderedDocument {
        RenderedDocument::EscPos(EscPosDocument {
            bytes: vec![
                0x1b, b'@', b'R', b'e', b'c', b'e', b'i', b'p', b't', b'\n', 0x1d, b'V', 0,
            ],
            receipt_width: oppa_protocol::ReceiptWidth::Mm80,
        })
    }

    #[tokio::test]
    async fn virtual_fail_next_resets_and_keeps_bounded_history() {
        let spooler = VirtualSpooler::new(1, DEFAULT_MAX_SUBMISSION_BYTES).expect("spooler");
        spooler.set_simulation(VirtualSimulation::FailNext).await;
        let printer = virtual_printer();
        let job = job_id();
        let document = virtual_document();
        let request = || SubmissionRequest {
            job_id: &job,
            printer: &printer,
            document: &document,
        };
        assert!(matches!(
            spooler.submit(request(), &CancellationToken::new()).await,
            Err(SpoolerError::SimulatedFailure)
        ));
        spooler
            .submit(request(), &CancellationToken::new())
            .await
            .expect("second attempt succeeds");
        let history = spooler.history().await;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].outcome, VirtualOutcome::Submitted);
        assert_eq!(
            (
                history[0].preview.pages[0].width,
                history[0].preview.pages[0].height
            ),
            (2480, 3508)
        );
        assert!(
            history[0].preview.pages[0]
                .data
                .iter()
                .any(|byte| *byte != 0)
        );
        let RenderedDocument::Raster(submitted_page) = &history[0].document else {
            panic!("virtual office device must capture the generic page raster");
        };
        assert_eq!(history[0].preview.pages, submitted_page.pages);
    }

    #[tokio::test]
    async fn virtual_thermal_consumes_escpos_bytes_and_interprets_receipt_semantics() {
        let printer = thermal_printer(58);
        let source = oppa_protocol_fixture();
        let document = DocumentRenderer::default()
            .render(&source, RenderTarget::EscPos)
            .expect("ESC/POS bytes");
        let spooler = VirtualSpooler::default();
        let job = job_id();
        spooler
            .submit(
                SubmissionRequest {
                    job_id: &job,
                    printer: &printer,
                    document: &document,
                },
                &CancellationToken::new(),
            )
            .await
            .expect("virtual thermal output");
        let history = spooler.history().await;
        let output = history.first().expect("captured receipt");
        assert!(matches!(output.document, RenderedDocument::EscPos(_)));
        assert_eq!(output.preview.pages[0].width, 464);
        assert!(output.preview.pages[0].data.iter().any(|byte| *byte != 0));
        let diagnostics = output.preview.escpos.as_ref().expect("interpreter details");
        assert_eq!(diagnostics.images, 1);
        assert_eq!(diagnostics.qr_codes, 1);
        assert_eq!(diagnostics.barcodes, 1);
        assert_eq!(diagnostics.feed_lines, 2);
        assert!(diagnostics.cut_requested);
        assert!(diagnostics.unsupported_commands.is_empty());
    }

    #[tokio::test]
    async fn raw_escpos_compatibility_page_matches_structured_page_output() {
        let source = oppa_protocol_fixture();
        let renderer = DocumentRenderer::default();
        let structured = renderer
            .render(&source, RenderTarget::Page(PageRenderOptions::A4_PORTRAIT))
            .expect("structured office page");
        let raw = renderer
            .render(&source, RenderTarget::EscPos)
            .expect("ESC/POS compatibility input");
        let printer = virtual_printer();
        let spooler = VirtualSpooler::default();
        let job = job_id();
        spooler
            .submit(
                SubmissionRequest {
                    job_id: &job,
                    printer: &printer,
                    document: &raw,
                },
                &CancellationToken::new(),
            )
            .await
            .expect("office compatibility output");
        let history = spooler.history().await;
        let compat_page = &history[0].preview.pages[0];
        let RenderedDocument::Raster(structured) = structured else {
            panic!("expected structured page raster");
        };
        let expected_page = &structured.pages[0];
        let differing_bytes = compat_page
            .data
            .iter()
            .zip(&expected_page.data)
            .filter(|(actual, expected)| actual != expected)
            .count();
        assert_eq!(differing_bytes, 0);
        assert!(history[0].preview.escpos.as_ref().unwrap().cut_requested);
    }

    #[tokio::test]
    async fn virtual_delay_supports_cancellation() {
        let spooler = VirtualSpooler::new(10, DEFAULT_MAX_SUBMISSION_BYTES).expect("spooler");
        spooler
            .set_simulation(VirtualSimulation::Delay(Duration::from_secs(30)))
            .await;
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let printer = virtual_printer();
        let job = job_id();
        let document = virtual_document();
        assert!(matches!(
            spooler
                .submit(
                    SubmissionRequest {
                        job_id: &job,
                        printer: &printer,
                        document: &document,
                    },
                    &cancellation,
                )
                .await,
            Err(SpoolerError::Cancelled)
        ));
        assert!(spooler.history().await.is_empty());
    }

    #[tokio::test]
    async fn raw_tcp_sends_all_escpos_bytes() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept");
            let mut bytes = Vec::new();
            tokio::io::AsyncReadExt::read_to_end(&mut stream, &mut bytes)
                .await
                .expect("read");
            bytes
        });
        let printer = PrinterRef {
            id: PrinterId::new("tcp_1").expect("printer"),
            display_name: "TCP receipt".to_owned(),
            connection: PrinterConnection::Network {
                host: "127.0.0.1".to_owned(),
                port: address.port(),
            },
            submission_mode: SubmissionMode::Raw(PrinterLanguage::EscPos),
            virtual_profile: None,
            enabled: true,
        };
        let job = job_id();
        let document = RenderedDocument::EscPos(oppa_renderer::EscPosDocument {
            bytes: vec![0x1b, b'@', b'O', b'K'],
            receipt_width: oppa_protocol::ReceiptWidth::Mm80,
        });
        RawTcpSpooler::default()
            .submit(
                SubmissionRequest {
                    job_id: &job,
                    printer: &printer,
                    document: &document,
                },
                &CancellationToken::new(),
            )
            .await
            .expect("submit");
        assert_eq!(server.await.expect("server"), vec![0x1b, b'@', b'O', b'K']);
    }

    #[test]
    fn generic_system_queue_prepares_driver_page_not_raw_bytes() {
        let printer = system_queue(SubmissionMode::Driver);
        let raw_document = escpos_document();
        let prepared =
            prepare_system_queue_submission(&printer, &raw_document, DEFAULT_MAX_SUBMISSION_BYTES)
                .expect("driver compatibility page");
        let SystemQueuePayload::DriverPages {
            document,
            compatibility,
        } = prepared
        else {
            panic!("generic system queue must never receive raw ESC/POS");
        };
        assert_eq!(document.pages.len(), 1);
        assert_eq!(
            (document.pages[0].width, document.pages[0].height),
            (2480, 3508)
        );
        assert!(document.pages[0].data.iter().any(|byte| *byte != 0));
        assert!(
            compatibility
                .expect("interpreter diagnostics")
                .cut_requested
        );
    }

    #[test]
    fn explicitly_raw_system_queue_keeps_printer_ready_escpos_bytes() {
        let printer = system_queue(SubmissionMode::Raw(PrinterLanguage::EscPos));
        let source = escpos_document();
        let prepared =
            prepare_system_queue_submission(&printer, &source, DEFAULT_MAX_SUBMISSION_BYTES)
                .expect("raw bytes");
        let SystemQueuePayload::RawEscPos(bytes) = prepared else {
            panic!("explicit raw queue should retain ESC/POS bytes");
        };
        assert_eq!(bytes, raw_bytes(&source).expect("ESC/POS bytes"));
    }

    #[test]
    fn cups_arguments_keep_driver_and_raw_filtering_separate() {
        #[cfg(unix)]
        {
            let raw = cups_raw_args("Thermal");
            assert_eq!(
                raw.iter()
                    .map(|arg| arg.to_string_lossy())
                    .collect::<Vec<_>>(),
                ["-d", "Thermal", "-o", "raw"]
            );
            let driver = cups_driver_args("Office", &[PathBuf::from("/tmp/page.png")]);
            let driver = driver
                .iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            assert_eq!(driver, ["-d", "Office", "/tmp/page.png"]);
            assert!(!driver.iter().any(|arg| arg == "raw"));
        }
    }

    #[test]
    fn driver_compatibility_stops_on_unknown_commands() {
        let printer = system_queue(SubmissionMode::Driver);
        let unsupported = RenderedDocument::EscPos(EscPosDocument {
            bytes: vec![0x1b, b'@', 0x1b, b'X', b'R', b'A', b'W'],
            receipt_width: oppa_protocol::ReceiptWidth::Mm80,
        });
        assert!(matches!(
            prepare_system_queue_submission(&printer, &unsupported, DEFAULT_MAX_SUBMISSION_BYTES),
            Err(SpoolerError::DocumentRender(_))
        ));
    }

    #[tokio::test]
    async fn raw_backend_rejects_virtual_documents_explicitly() {
        let printer = PrinterRef {
            id: PrinterId::new("tcp_1").expect("printer"),
            display_name: "TCP receipt".to_owned(),
            connection: PrinterConnection::Network {
                host: "127.0.0.1".to_owned(),
                port: 9100,
            },
            submission_mode: SubmissionMode::Raw(PrinterLanguage::EscPos),
            virtual_profile: None,
            enabled: true,
        };
        let job = job_id();
        let document = legacy_virtual_document();
        assert!(matches!(
            RawTcpSpooler::default()
                .submit(
                    SubmissionRequest {
                        job_id: &job,
                        printer: &printer,
                        document: &document,
                    },
                    &CancellationToken::new(),
                )
                .await,
            Err(SpoolerError::UnsupportedDocument { .. })
        ));
    }
}
