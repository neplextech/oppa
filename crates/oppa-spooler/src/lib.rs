//! Concrete printer submission transports with explicit deadlines.
//!
//! A successful [`SubmissionReceipt`] means a backend accepted all bytes. It
//! does not claim that paper was physically produced.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    process::Stdio,
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use oppa_core::{PrintJobId, PrinterId, Timestamp};
use oppa_printer::{ConnectionKind, PrinterConnection, PrinterRef, SubmissionReceipt};
use oppa_renderer::RenderedDocument;
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
            BTreeMap::from([("endpoint".to_owned(), format!("{host}:{port}"))]),
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
        let bytes = raw_bytes(request.document)?;
        enforce_size(bytes.len(), self.max_submission_bytes)?;
        submit_system_queue(queue_name, bytes, self.submission_timeout, cancellation).await
    }
}

#[cfg(unix)]
async fn submit_system_queue(
    queue_name: &str,
    bytes: &[u8],
    deadline: Duration,
    cancellation: &CancellationToken,
) -> SpoolerResult<SubmissionReceipt> {
    let mut child = tokio::process::Command::new("lp")
        .args(["-d", queue_name, "-o", "raw"])
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
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SpoolerError::Rejected(
                stderr.trim().chars().take(1_000).collect(),
            ));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let backend_job_id = parse_lp_job_id(&stdout);
        Ok(receipt(
            "system-queue",
            backend_job_id,
            BTreeMap::from([("queue".to_owned(), queue_name.to_owned())]),
        ))
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

#[cfg(not(unix))]
async fn submit_system_queue(
    _queue_name: &str,
    _bytes: &[u8],
    _deadline: Duration,
    _cancellation: &CancellationToken,
) -> SpoolerResult<SubmissionReceipt> {
    Err(SpoolerError::BackendUnavailable(
        "system queue submission is not implemented on this platform".to_owned(),
    ))
}

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
        RenderedDocument::EscPos(bytes) => Ok(bytes),
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
            max_history: 100,
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
        })
        .await;

        match outcome {
            VirtualOutcome::Submitted => {
                let backend_job_id = Uuid::new_v4().to_string();
                Ok(receipt("virtual", Some(backend_job_id), BTreeMap::new()))
            }
            VirtualOutcome::Failed => Err(SpoolerError::SimulatedFailure),
            VirtualOutcome::Offline => Err(SpoolerError::Connectivity(
                "virtual printer is offline".to_owned(),
            )),
        }
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
    use oppa_renderer::VirtualPrintDocument;

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
            enabled: true,
        }
    }

    fn virtual_document() -> RenderedDocument {
        RenderedDocument::Virtual(VirtualPrintDocument {
            document: oppa_protocol_fixture(),
            preview_lines: vec!["Test".to_owned()],
        })
    }

    fn oppa_protocol_fixture() -> oppa_protocol::PrintDocument {
        oppa_protocol::PrintDocument {
            width: oppa_protocol::ReceiptWidth::Mm58,
            sections: vec![oppa_protocol::PrintSection::Text {
                value: "Test".to_owned(),
                align: None,
                bold: None,
            }],
        }
    }

    #[tokio::test]
    async fn virtual_fail_next_resets_and_keeps_bounded_history() {
        let spooler = VirtualSpooler::new(1, 1024).expect("spooler");
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
    }

    #[tokio::test]
    async fn virtual_delay_supports_cancellation() {
        let spooler = VirtualSpooler::new(10, 1024).expect("spooler");
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
            enabled: true,
        };
        let job = job_id();
        let document = RenderedDocument::EscPos(vec![0x1b, b'@', b'O', b'K']);
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

    #[tokio::test]
    async fn raw_backend_rejects_virtual_documents_explicitly() {
        let printer = PrinterRef {
            id: PrinterId::new("tcp_1").expect("printer"),
            display_name: "TCP receipt".to_owned(),
            connection: PrinterConnection::Network {
                host: "127.0.0.1".to_owned(),
                port: 9100,
            },
            enabled: true,
        };
        let job = job_id();
        let document = virtual_document();
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
