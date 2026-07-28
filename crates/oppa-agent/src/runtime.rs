use std::sync::Arc;

use async_trait::async_trait;
use oppa_core::PrintJobId;
use oppa_printer::SubmissionReceipt;
use oppa_protocol::{
    AgentMessage, AgentMessageKind, PrintJob, ServerMessage, ServerMessageKind, Validate,
};
use oppa_renderer::DocumentRenderer;
use oppa_spooler::SpoolerRegistry;
use oppa_storage::{DEFAULT_OUTBOUND_STATUS_BATCH_SIZE, JobRepository};
use thiserror::Error;
use tokio::sync::Mutex;

use crate::{
    AgentHandle, AgentSnapshot, CancelOutcome, JobFlowResult, JobProcessingError, JobProcessor,
    LocalTestPrintError, PrinterResolver, ProcessOutcome, RecoverySummary,
};

/// Bounded error returned by the host's outbound protocol transport.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("outbound protocol report failed: {message}")]
pub struct OutboundReportError {
    message: String,
}

impl OutboundReportError {
    /// Creates an error without retaining credentials or print payloads.
    #[must_use]
    pub fn new(message: impl AsRef<str>) -> Self {
        let message: String = message
            .as_ref()
            .chars()
            .filter(|character| !character.is_control())
            .take(1_000)
            .collect();
        Self {
            message: if message.trim().is_empty() {
                "transport did not provide diagnostic details".to_owned()
            } else {
                message
            },
        }
    }

    /// Returns the sanitized transport diagnostic.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Transport-owned boundary for sending validated agent status messages.
#[async_trait]
pub trait OutboundReporter: Send + Sync {
    /// Sends one already validated message or returns a retryable host error.
    async fn report(&self, message: &AgentMessage) -> Result<(), OutboundReportError>;
}

/// Missing dependency reported while assembling an [`Agent`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum AgentBuildError {
    /// No durable job repository was supplied.
    #[error("agent builder requires a job repository")]
    MissingRepository,
    /// No configured-printer resolver was supplied.
    #[error("agent builder requires a printer resolver")]
    MissingPrinterResolver,
    /// No spooler registry was supplied.
    #[error("agent builder requires a spooler registry")]
    MissingSpoolerRegistry,
    /// No outbound status reporter was supplied.
    #[error("agent builder requires an outbound reporter")]
    MissingOutboundReporter,
}

/// Failure while coordinating durable work with outbound reporting.
#[derive(Debug, Error)]
pub enum AgentRuntimeError {
    /// Durable job processing failed.
    #[error(transparent)]
    Processing(#[from] JobProcessingError),
    /// A non-job server message was passed to the job runtime.
    #[error("agent job runtime does not handle {message_type}")]
    UnsupportedServerMessage {
        /// Stable server message discriminator.
        message_type: &'static str,
    },
    /// A durable acknowledgement or result could not be sent.
    ///
    /// The validated message is retained so a transport adapter can retry it.
    #[error("could not report durable agent status: {source}")]
    Reporting {
        /// Validated message whose durable transition already occurred.
        message: AgentMessage,
        /// Sanitized transport failure.
        #[source]
        source: OutboundReportError,
    },
    /// A durable outbox row did not contain its expected validated status.
    #[error("durable outbound status {message_id} is invalid: {reason}")]
    InvalidOutboundStatus {
        /// Stable outbox identity.
        message_id: String,
        /// Payload-free validation reason.
        reason: &'static str,
    },
}

/// Result of dispatching a supported server job command.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum ServerJobOutcome {
    /// A print job was durably received and, if new, processed.
    PrintJob(JobFlowResult),
    /// A cancellation request was applied or routed cooperatively.
    CancelJob(CancelOutcome),
}

/// Builder for the shell-independent durable agent runtime.
pub struct AgentBuilder {
    repository: Option<Arc<dyn JobRepository>>,
    printers: Option<Arc<dyn PrinterResolver>>,
    renderer: DocumentRenderer,
    spoolers: Option<Arc<SpoolerRegistry>>,
    reporter: Option<Arc<dyn OutboundReporter>>,
    handle: AgentHandle,
}

impl Default for AgentBuilder {
    fn default() -> Self {
        Self {
            repository: None,
            printers: None,
            renderer: DocumentRenderer::default(),
            spoolers: None,
            reporter: None,
            handle: AgentHandle::new(AgentSnapshot::default()),
        }
    }
}

impl AgentBuilder {
    /// Creates a builder with the default renderer and lifecycle snapshot.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Supplies durable local job storage.
    #[must_use]
    pub fn repository(mut self, repository: Arc<dyn JobRepository>) -> Self {
        self.repository = Some(repository);
        self
    }

    /// Supplies configured local-printer resolution.
    #[must_use]
    pub fn printer_resolver(mut self, printers: Arc<dyn PrinterResolver>) -> Self {
        self.printers = Some(printers);
        self
    }

    /// Overrides the structured-document renderer.
    #[must_use]
    pub fn renderer(mut self, renderer: DocumentRenderer) -> Self {
        self.renderer = renderer;
        self
    }

    /// Supplies concrete spoolers keyed by connection family.
    #[must_use]
    pub fn spooler_registry(mut self, spoolers: Arc<SpoolerRegistry>) -> Self {
        self.spoolers = Some(spoolers);
        self
    }

    /// Supplies the transport-owned outbound status boundary.
    #[must_use]
    pub fn outbound_reporter(mut self, reporter: Arc<dyn OutboundReporter>) -> Self {
        self.reporter = Some(reporter);
        self
    }

    /// Reuses an existing lifecycle handle and its subscriptions.
    #[must_use]
    pub fn handle(mut self, handle: AgentHandle) -> Self {
        self.handle = handle;
        self
    }

    /// Validates required dependencies and constructs the runtime.
    ///
    /// # Errors
    ///
    /// Returns the first missing required infrastructure boundary.
    pub fn build(self) -> Result<Agent, AgentBuildError> {
        let repository = self.repository.ok_or(AgentBuildError::MissingRepository)?;
        let printers = self
            .printers
            .ok_or(AgentBuildError::MissingPrinterResolver)?;
        let spoolers = self
            .spoolers
            .ok_or(AgentBuildError::MissingSpoolerRegistry)?;
        let reporter = self
            .reporter
            .ok_or(AgentBuildError::MissingOutboundReporter)?;
        let processor = Arc::new(JobProcessor::new(
            Arc::clone(&repository),
            printers,
            self.renderer,
            spoolers,
            self.handle.clone(),
        ));
        Ok(Agent {
            handle: self.handle,
            processor,
            reporter,
            repository,
            flow_lock: Mutex::new(()),
            outbox_lock: Mutex::new(()),
        })
    }
}

/// Shell-independent OPPA runtime for durable server print-job commands.
pub struct Agent {
    handle: AgentHandle,
    processor: Arc<JobProcessor>,
    reporter: Arc<dyn OutboundReporter>,
    repository: Arc<dyn JobRepository>,
    flow_lock: Mutex<()>,
    outbox_lock: Mutex<()>,
}

impl Agent {
    /// Returns the cloneable lifecycle and event handle.
    #[must_use]
    pub fn handle(&self) -> AgentHandle {
        self.handle.clone()
    }

    /// Returns the focused job processor for advanced host orchestration.
    #[must_use]
    pub fn job_processor(&self) -> Arc<JobProcessor> {
        Arc::clone(&self.processor)
    }

    /// Submits host-owned test content without durable server-job or outbox
    /// state.
    ///
    /// # Errors
    ///
    /// Returns a bounded error when validation, printer resolution, rendering,
    /// or spooler submission fails.
    pub async fn submit_local_test(
        &self,
        job: &PrintJob,
    ) -> Result<SubmissionReceipt, LocalTestPrintError> {
        let _flow = self.flow_lock.lock().await;
        self.processor.submit_local_test(job).await
    }

    /// Dispatches a print-job or cancellation server message.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid or unsupported protocol messages, durable
    /// processing failures, or outbound reporting failures.
    pub async fn handle_server_message(
        &self,
        message: &ServerMessage,
    ) -> Result<ServerJobOutcome, AgentRuntimeError> {
        message.validate().map_err(JobProcessingError::from)?;
        match &message.kind {
            ServerMessageKind::PrintJob(_) => self
                .handle_print_job(message)
                .await
                .map(ServerJobOutcome::PrintJob),
            ServerMessageKind::CancelJob(cancel) => self
                .cancel_job(&cancel.job_id)
                .await
                .map(ServerJobOutcome::CancelJob),
            kind => Err(AgentRuntimeError::UnsupportedServerMessage {
                message_type: kind.message_type(),
            }),
        }
    }

    /// Persists, acknowledges, and processes one at-least-once job delivery.
    ///
    /// The receipt report is awaited before a newly inserted job is claimed.
    /// Terminal reports are sent only after their database transition commits.
    ///
    /// # Errors
    ///
    /// Returns an error when receipt or processing fails, or when the outbound
    /// reporter cannot send a status. Reporting errors retain the message.
    pub async fn handle_print_job(
        &self,
        message: &ServerMessage,
    ) -> Result<JobFlowResult, AgentRuntimeError> {
        let _flow = self.flow_lock.lock().await;
        let receipt = self.processor.receive_print_job(message).await?;
        if let Some(acknowledgement) = receipt.acknowledgement() {
            self.report(acknowledgement).await?;
        }

        let outcomes = if receipt.was_inserted() {
            let outcomes = self.processor.process_pending().await?;
            self.report_outcomes(&outcomes).await?;
            outcomes
        } else {
            Vec::new()
        };
        Ok(JobFlowResult { receipt, outcomes })
    }

    /// Processes already received jobs and reports each durable terminal result.
    ///
    /// # Errors
    ///
    /// Returns an error when durable processing or outbound reporting fails.
    pub async fn process_pending(&self) -> Result<Vec<ProcessOutcome>, AgentRuntimeError> {
        let _flow = self.flow_lock.lock().await;
        self.report_pending_receipts().await?;
        let outcomes = self.processor.process_pending().await?;
        self.report_outcomes(&outcomes).await?;
        Ok(outcomes)
    }

    /// Recovers interrupted submissions, re-acknowledges pending work, and
    /// reports newly produced terminal results.
    ///
    /// A connected host must call [`Self::replay_outbound_reports`] after the
    /// `agent.hello` handshake and before this method. Keeping durable terminal
    /// replay explicit prevents a transport from emitting application messages
    /// before the required hello.
    ///
    /// # Errors
    ///
    /// Returns an error when recovery, receipt replay, processing, or outbound
    /// reporting fails.
    pub async fn recover(&self) -> Result<RecoverySummary, AgentRuntimeError> {
        let _flow = self.flow_lock.lock().await;
        let recovery = self.processor.recover_interrupted().await?;
        self.report_pending_receipts().await?;
        let outcomes = self.processor.process_pending().await?;
        let summary = RecoverySummary { recovery, outcomes };
        self.report_outcomes(&summary.outcomes).await?;
        Ok(summary)
    }

    /// Replays durable terminal statuses left unacknowledged by an earlier
    /// transport session.
    ///
    /// # Errors
    ///
    /// Returns an error when the outbox cannot be read, contains an invalid
    /// status, or outbound reporting fails. Successfully delivered rows are
    /// acknowledged one at a time.
    pub async fn replay_outbound_reports(&self) -> Result<usize, AgentRuntimeError> {
        let _replay = self.outbox_lock.lock().await;
        let mut delivered = 0_usize;
        loop {
            let statuses = self
                .repository
                .pending_outbound_statuses(DEFAULT_OUTBOUND_STATUS_BATCH_SIZE)
                .await
                .map_err(JobProcessingError::from)?;
            if statuses.is_empty() {
                return Ok(delivered);
            }
            let batch_len = statuses.len();
            for status in statuses {
                let message: AgentMessage =
                    serde_json::from_value(status.payload).map_err(JobProcessingError::from)?;
                message.validate().map_err(JobProcessingError::from)?;
                validate_outbound_status(&status.message_id, &status.job_id, &message).map_err(
                    |reason| AgentRuntimeError::InvalidOutboundStatus {
                        message_id: status.message_id.clone(),
                        reason,
                    },
                )?;
                self.report_and_ack_unlocked(&message).await?;
                delivered = delivered.saturating_add(1);
            }
            if batch_len < DEFAULT_OUTBOUND_STATUS_BATCH_SIZE {
                return Ok(delivered);
            }
        }
    }

    /// Cancels queued work or signals an active spooler cooperatively.
    ///
    /// # Errors
    ///
    /// Returns an error when the job ID is invalid or durable state cannot be
    /// inspected or transitioned.
    pub async fn cancel_job(
        &self,
        job_id: impl AsRef<str>,
    ) -> Result<CancelOutcome, AgentRuntimeError> {
        self.processor
            .cancel_job(job_id)
            .await
            .map_err(AgentRuntimeError::from)
    }

    async fn report_outcomes(&self, outcomes: &[ProcessOutcome]) -> Result<(), AgentRuntimeError> {
        let _outbox = self.outbox_lock.lock().await;
        for outcome in outcomes {
            if let Some(message) = outcome.message() {
                self.report_and_ack_unlocked(message).await?;
            }
        }
        Ok(())
    }

    async fn report_pending_receipts(&self) -> Result<(), AgentRuntimeError> {
        for message in self.processor.pending_receipts().await? {
            self.report(&message).await?;
        }
        Ok(())
    }

    async fn report(&self, message: &AgentMessage) -> Result<(), AgentRuntimeError> {
        self.reporter
            .report(message)
            .await
            .map_err(|source| AgentRuntimeError::Reporting {
                message: message.clone(),
                source,
            })
    }

    async fn report_and_ack_unlocked(
        &self,
        message: &AgentMessage,
    ) -> Result<(), AgentRuntimeError> {
        self.report(message).await?;
        self.repository
            .acknowledge_outbound_status(&message.message_id)
            .await
            .map_err(JobProcessingError::from)?;
        Ok(())
    }
}

fn validate_outbound_status(
    stored_message_id: &str,
    stored_job_id: &PrintJobId,
    message: &AgentMessage,
) -> Result<(), &'static str> {
    if message.message_id != stored_message_id {
        return Err("message identity does not match its outbox row");
    }
    let payload_job_id = match &message.kind {
        AgentMessageKind::JobSubmitted(payload) => &payload.job_id,
        AgentMessageKind::JobFailed(payload) => &payload.job_id,
        _ => return Err("outbox payload is not a terminal job status"),
    };
    if payload_job_id != stored_job_id.as_str() {
        return Err("job identity does not match its outbox row");
    }
    Ok(())
}
