use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use oppa_core::{IdentifierError, JobState, PrintJobId, PrinterId, Timestamp};
use oppa_printer::{PrinterConnection, PrinterRef, SubmissionReceipt};
use oppa_protocol::{
    AgentMessage, AgentMessageKind, FailureDetail, JobFailed, JobReceived, JobStatus, JobSubmitted,
    PrintJob, ProtocolVersion, ServerMessage, ServerMessageKind, Validate, ValidationError,
};
use oppa_renderer::{DocumentRenderer, RenderTarget, RenderedDocument, RendererError};
use oppa_spooler::{SpoolerError, SpoolerRegistry, SubmissionRequest};
use oppa_storage::{
    InsertResult, JobRepository, NewOutboundStatus, ReceivedPrintJob, RecoveryResult, StorageError,
    StoredJobError, StoredPrintJob,
};
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::warn;
use uuid::Uuid;

use crate::{AgentEvent, AgentHandle};

/// Failure returned by a configured-printer resolver.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PrinterResolutionError {
    /// A printer reference failed local validation.
    #[error("invalid printer reference: {0}")]
    InvalidPrinter(String),
    /// A static catalog contained the same stable ID more than once.
    #[error("duplicate printer id {0}")]
    DuplicatePrinter(PrinterId),
    /// A local resolver backend failed.
    #[error("printer resolver failed: {0}")]
    Backend(String),
}

impl PrinterResolutionError {
    /// Creates a bounded backend error without retaining printer payloads.
    #[must_use]
    pub fn backend(message: impl AsRef<str>) -> Self {
        Self::Backend(sanitize_message(message.as_ref()))
    }
}

/// Resolves a concrete local printer selected by an already-routed job.
#[async_trait]
pub trait PrinterResolver: Send + Sync {
    /// Returns the current configured reference, or `None` when it is unknown.
    async fn resolve(
        &self,
        printer_id: &PrinterId,
    ) -> Result<Option<PrinterRef>, PrinterResolutionError>;
}

/// Mutable in-memory printer catalog suitable for hosts and tests.
#[derive(Default)]
pub struct StaticPrinterResolver {
    printers: RwLock<HashMap<PrinterId, PrinterRef>>,
}

impl StaticPrinterResolver {
    /// Builds a catalog after validating every reference and stable ID.
    ///
    /// # Errors
    ///
    /// Returns an error when a printer is invalid or the input contains the
    /// same stable printer ID more than once.
    pub fn new(
        printers: impl IntoIterator<Item = PrinterRef>,
    ) -> Result<Self, PrinterResolutionError> {
        let mut catalog = HashMap::new();
        for printer in printers {
            printer
                .validate()
                .map_err(|error| PrinterResolutionError::InvalidPrinter(error.to_string()))?;
            let id = printer.id.clone();
            if catalog.insert(id.clone(), printer).is_some() {
                return Err(PrinterResolutionError::DuplicatePrinter(id));
            }
        }
        Ok(Self {
            printers: RwLock::new(catalog),
        })
    }

    /// Adds or replaces one validated printer reference.
    ///
    /// # Errors
    ///
    /// Returns [`PrinterResolutionError::InvalidPrinter`] when the reference
    /// cannot be used by a spooler.
    pub async fn upsert(&self, printer: PrinterRef) -> Result<(), PrinterResolutionError> {
        printer
            .validate()
            .map_err(|error| PrinterResolutionError::InvalidPrinter(error.to_string()))?;
        self.printers
            .write()
            .await
            .insert(printer.id.clone(), printer);
        Ok(())
    }

    /// Removes a printer reference and returns whether it existed.
    pub async fn remove(&self, printer_id: &PrinterId) -> bool {
        self.printers.write().await.remove(printer_id).is_some()
    }
}

#[async_trait]
impl PrinterResolver for StaticPrinterResolver {
    async fn resolve(
        &self,
        printer_id: &PrinterId,
    ) -> Result<Option<PrinterRef>, PrinterResolutionError> {
        Ok(self.printers.read().await.get(printer_id).cloned())
    }
}

/// Failure at a durable job-processing boundary.
#[derive(Debug, Error)]
pub enum JobProcessingError {
    /// A non-print-job server message was passed to the receipt path.
    #[error("expected a server.print_job message")]
    ExpectedPrintJob,
    /// A constructed or inbound protocol value violated the canonical schema.
    #[error("protocol validation failed: {0}")]
    Protocol(#[from] ValidationError),
    /// A protocol identifier could not be represented by the local domain type.
    #[error("invalid local identifier: {0}")]
    Identifier(#[from] IdentifierError),
    /// A validated payload could not be represented as JSON.
    #[error("could not serialize durable print job: {0}")]
    Json(#[from] serde_json::Error),
    /// Durable storage failed.
    #[error(transparent)]
    Storage(#[from] StorageError),
}

/// Bounded failure from a locally initiated, non-durable test submission.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("local test print failed ({code}): {message}")]
pub struct LocalTestPrintError {
    code: &'static str,
    message: String,
    retryable: bool,
}

impl LocalTestPrintError {
    fn from_failure(failure: JobFailure) -> Self {
        Self {
            code: failure.code,
            message: failure.message,
            retryable: failure.retryable,
        }
    }

    /// Returns the stable machine-readable failure category.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }

    /// Returns whether repeating the local test may succeed later.
    #[must_use]
    pub const fn retryable(&self) -> bool {
        self.retryable
    }

    /// Returns the bounded, sanitized diagnostic.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Durable receipt outcome for an at-least-once job delivery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReceiveJobOutcome {
    /// A new row was committed; the acknowledgement may now be sent.
    Inserted {
        /// Persist-after-commit acknowledgement.
        acknowledgement: AgentMessage,
    },
    /// The exact job was already durable and is re-acknowledged without work.
    DuplicateJob {
        /// Current durable lifecycle.
        state: JobState,
        /// Acknowledgement for the redelivered job.
        acknowledgement: AgentMessage,
    },
    /// Another durable job already owns the idempotency key.
    DuplicateIdempotency {
        /// Incoming job that was not persisted.
        job_id: PrintJobId,
        /// Existing job that owns the key.
        existing_job_id: PrintJobId,
        /// Existing durable lifecycle.
        state: JobState,
    },
}

impl ReceiveJobOutcome {
    /// Returns whether this delivery created new durable work.
    #[must_use]
    pub const fn was_inserted(&self) -> bool {
        matches!(self, Self::Inserted { .. })
    }

    /// Returns the safe receipt acknowledgement, when one is warranted.
    #[must_use]
    pub const fn acknowledgement(&self) -> Option<&AgentMessage> {
        match self {
            Self::Inserted { acknowledgement }
            | Self::DuplicateJob {
                acknowledgement, ..
            } => Some(acknowledgement),
            Self::DuplicateIdempotency { .. } => None,
        }
    }
}

/// Terminal or skipped result from processing one durable pending job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessOutcome {
    /// Backend acceptance was persisted before the status message was built.
    Submitted {
        /// Persisted job identity.
        job_id: PrintJobId,
        /// Correlated `agent.job_submitted` report.
        message: AgentMessage,
    },
    /// A failure was persisted before any optional status report.
    Failed {
        /// Persisted job identity.
        job_id: PrintJobId,
        /// Correlated report, absent only when persisted recovery metadata was corrupt.
        message: Option<AgentMessage>,
    },
    /// Cooperative cancellation reached durable state before backend acceptance.
    Cancelled {
        /// Persisted job identity.
        job_id: PrintJobId,
    },
    /// Another processor or cancellation already changed the durable state.
    Skipped {
        /// Persisted job identity.
        job_id: PrintJobId,
        /// State observed when the claim failed.
        state: JobState,
    },
}

impl ProcessOutcome {
    /// Returns the terminal wire report, when protocol correlation is available.
    #[must_use]
    pub const fn message(&self) -> Option<&AgentMessage> {
        match self {
            Self::Submitted { message, .. } => Some(message),
            Self::Failed { message, .. } => message.as_ref(),
            Self::Cancelled { .. } | Self::Skipped { .. } => None,
        }
    }

    /// Returns the durable job identity.
    #[must_use]
    pub const fn job_id(&self) -> &PrintJobId {
        match self {
            Self::Submitted { job_id, .. }
            | Self::Failed { job_id, .. }
            | Self::Cancelled { job_id }
            | Self::Skipped { job_id, .. } => job_id,
        }
    }
}

/// Result returned after receipt acknowledgement and newly triggered work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobFlowResult {
    /// Durable receipt or duplicate outcome.
    pub receipt: ReceiveJobOutcome,
    /// Terminal outcomes produced after a new receipt was acknowledged.
    pub outcomes: Vec<ProcessOutcome>,
}

/// Startup recovery and replay result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoverySummary {
    /// Durable recovery transition summary.
    pub recovery: RecoveryResult,
    /// Replay outcomes in oldest-first order.
    pub outcomes: Vec<ProcessOutcome>,
}

/// Result of a best-effort cancellation request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CancelOutcome {
    /// An active spooler received cooperative cancellation.
    CancellationRequested {
        /// Durable job identity.
        job_id: PrintJobId,
    },
    /// A queued or otherwise locally claimable job became durably cancelled.
    Cancelled {
        /// Durable job identity.
        job_id: PrintJobId,
    },
    /// The job was already terminal.
    AlreadyTerminal {
        /// Durable job identity.
        job_id: PrintJobId,
        /// Existing terminal state.
        state: JobState,
    },
    /// No durable job exists with this identity.
    NotFound {
        /// Requested job identity.
        job_id: PrintJobId,
    },
}

#[derive(Debug)]
struct JobFailure {
    code: &'static str,
    message: String,
    retryable: bool,
}

impl JobFailure {
    fn new(code: &'static str, message: impl AsRef<str>, retryable: bool) -> Self {
        Self {
            code,
            message: sanitize_message(message.as_ref()),
            retryable,
        }
    }

    fn stored(&self, occurred_at: Timestamp) -> StoredJobError {
        StoredJobError {
            code: self.code.to_owned(),
            message: self.message.clone(),
            retryable: self.retryable,
            occurred_at,
        }
    }
}

/// Durable print-job receipt, recovery, rendering, and submission coordinator.
pub struct JobProcessor {
    repository: Arc<dyn JobRepository>,
    printers: Arc<dyn PrinterResolver>,
    renderer: DocumentRenderer,
    spoolers: Arc<SpoolerRegistry>,
    handle: AgentHandle,
    active: Mutex<HashMap<PrintJobId, CancellationToken>>,
    drain_lock: Mutex<()>,
}

impl JobProcessor {
    /// Creates a processor from explicit infrastructure boundaries.
    #[must_use]
    pub fn new(
        repository: Arc<dyn JobRepository>,
        printers: Arc<dyn PrinterResolver>,
        renderer: DocumentRenderer,
        spoolers: Arc<SpoolerRegistry>,
        handle: AgentHandle,
    ) -> Self {
        Self {
            repository,
            printers,
            renderer,
            spoolers,
            handle,
            active: Mutex::new(HashMap::new()),
            drain_lock: Mutex::new(()),
        }
    }

    /// Validates and commits a delivered print job before returning its acknowledgement.
    ///
    /// Exact redelivery returns another receipt acknowledgement but never
    /// schedules another submission. Reuse of an idempotency key by a
    /// different job is reported as a conflict and is not acknowledged.
    ///
    /// # Errors
    ///
    /// Returns an error when protocol validation, identifier conversion,
    /// serialization, or the durable insertion fails.
    pub async fn receive_print_job(
        &self,
        message: &ServerMessage,
    ) -> Result<ReceiveJobOutcome, JobProcessingError> {
        message.validate()?;
        let ServerMessageKind::PrintJob(job) = &message.kind else {
            return Err(JobProcessingError::ExpectedPrintJob);
        };
        job.validate()?;

        let job_id = PrintJobId::new(job.job_id.clone())?;
        let printer_id = PrinterId::new(job.printer_id.clone())?;
        let received_at = Timestamp::now();
        let payload = serde_json::to_value(job)?;
        let received = ReceivedPrintJob {
            id: job_id.clone(),
            idempotency_key: job.idempotency_key.clone(),
            printer_id,
            payload,
            source_message_id: message.message_id.clone(),
            received_at,
        };

        match self.repository.insert_received(&received).await? {
            InsertResult::Inserted => {
                self.handle.publish_job_event(AgentEvent::JobReceived {
                    job_id: job_id.clone(),
                });
                self.refresh_pending_best_effort().await;
                Ok(ReceiveJobOutcome::Inserted {
                    acknowledgement: received_message(job, &message.message_id, received_at)?,
                })
            }
            InsertResult::DuplicateJob { state } => {
                self.handle
                    .publish_job_event(AgentEvent::DuplicateJob { job_id, state });
                Ok(ReceiveJobOutcome::DuplicateJob {
                    state,
                    acknowledgement: received_message(job, &message.message_id, Timestamp::now())?,
                })
            }
            InsertResult::DuplicateIdempotency {
                existing_job_id,
                state,
            } => {
                self.handle
                    .publish_job_event(AgentEvent::DuplicateIdempotency {
                        job_id: job_id.clone(),
                        existing_job_id: existing_job_id.clone(),
                        state,
                    });
                Ok(ReceiveJobOutcome::DuplicateIdempotency {
                    job_id,
                    existing_job_id,
                    state,
                })
            }
        }
    }

    /// Validates, renders, and submits a locally initiated test job without
    /// creating durable server-job state or outbound protocol reports.
    ///
    /// This path is only for host-owned test content. Server deliveries must
    /// use [`Self::receive_print_job`] followed by [`Self::process_pending`] so
    /// their acknowledgement, idempotency, and recovery guarantees remain
    /// intact.
    ///
    /// # Errors
    ///
    /// Returns a bounded error when the job is invalid, the configured printer
    /// cannot be used, rendering fails, or the spooler rejects submission.
    pub async fn submit_local_test(
        &self,
        job: &PrintJob,
    ) -> Result<SubmissionReceipt, LocalTestPrintError> {
        job.validate().map_err(|error| {
            LocalTestPrintError::from_failure(JobFailure::new(
                "job.invalid",
                error.to_string(),
                false,
            ))
        })?;
        let job_id = PrintJobId::new(job.job_id.clone()).map_err(|error| {
            LocalTestPrintError::from_failure(JobFailure::new(
                "job.invalid_identifier",
                error.to_string(),
                false,
            ))
        })?;
        let printer_id = PrinterId::new(job.printer_id.clone()).map_err(|error| {
            LocalTestPrintError::from_failure(JobFailure::new(
                "printer.invalid_identifier",
                error.to_string(),
                false,
            ))
        })?;
        let printer = self
            .resolve_enabled_printer(&printer_id)
            .await
            .map_err(LocalTestPrintError::from_failure)?;
        let rendered = self
            .render(job, &printer)
            .map_err(LocalTestPrintError::from_failure)?;
        let cancellation = CancellationToken::new();
        self.spoolers
            .submit(
                SubmissionRequest {
                    job_id: &job_id,
                    printer: &printer,
                    document: &rendered,
                },
                &cancellation,
            )
            .await
            .map_err(|error| LocalTestPrintError::from_failure(spooler_failure(&error)))
    }

    /// Processes every currently received job in durable oldest-first order.
    ///
    /// # Errors
    ///
    /// Returns an error when durable state cannot be read or transitioned.
    pub async fn process_pending(&self) -> Result<Vec<ProcessOutcome>, JobProcessingError> {
        let _drain = self.drain_lock.lock().await;
        self.process_pending_inner().await
    }

    pub(crate) async fn pending_receipts(&self) -> Result<Vec<AgentMessage>, JobProcessingError> {
        self.repository
            .pending()
            .await?
            .iter()
            .map(received_message_for_stored)
            .collect::<Result<Vec<_>, _>>()
            .map_err(JobProcessingError::from)
    }

    pub(crate) async fn recover_interrupted(&self) -> Result<RecoveryResult, JobProcessingError> {
        let _drain = self.drain_lock.lock().await;
        self.recover_interrupted_inner().await
    }

    /// Restores interrupted submissions and replays every pending job.
    ///
    /// # Errors
    ///
    /// Returns an error when recovery or a durable lifecycle transition fails.
    pub async fn recover_and_process(&self) -> Result<RecoverySummary, JobProcessingError> {
        let _drain = self.drain_lock.lock().await;
        let recovery = self.recover_interrupted_inner().await?;
        let outcomes = self.process_pending_inner().await?;
        Ok(RecoverySummary { recovery, outcomes })
    }

    async fn recover_interrupted_inner(&self) -> Result<RecoveryResult, JobProcessingError> {
        let recovery = self.repository.recover_interrupted().await?;
        self.handle.set_pending_jobs(recovery.pending_jobs).await;
        self.handle
            .publish_job_event(AgentEvent::RecoveryCompleted {
                recovered_submissions: recovery.recovered_submissions,
                pending_jobs: recovery.pending_jobs,
            });
        Ok(recovery)
    }

    /// Cancels queued work immediately or signals an active spooler cooperatively.
    ///
    /// # Errors
    ///
    /// Returns an error when the identifier is invalid or durable cancellation
    /// cannot be evaluated.
    pub async fn cancel_job(
        &self,
        job_id: impl AsRef<str>,
    ) -> Result<CancelOutcome, JobProcessingError> {
        let job_id = PrintJobId::new(job_id.as_ref().to_owned())?;
        let active = self.active.lock().await;
        if let Some(cancellation) = active.get(&job_id) {
            cancellation.cancel();
            self.handle
                .publish_job_event(AgentEvent::CancellationRequested {
                    job_id: job_id.clone(),
                });
            return Ok(CancelOutcome::CancellationRequested { job_id });
        }

        let outcome = match self.repository.mark_cancelled(&job_id).await {
            Ok(()) => {
                self.handle.publish_job_event(AgentEvent::JobCancelled {
                    job_id: job_id.clone(),
                });
                CancelOutcome::Cancelled {
                    job_id: job_id.clone(),
                }
            }
            Err(StorageError::JobNotFound(_)) => CancelOutcome::NotFound {
                job_id: job_id.clone(),
            },
            Err(StorageError::InvalidTransition { current, .. }) if current.is_terminal() => {
                CancelOutcome::AlreadyTerminal {
                    job_id: job_id.clone(),
                    state: current,
                }
            }
            Err(error) => return Err(error.into()),
        };
        drop(active);
        self.refresh_pending_best_effort().await;
        Ok(outcome)
    }

    async fn process_pending_inner(&self) -> Result<Vec<ProcessOutcome>, JobProcessingError> {
        let pending = self.repository.pending().await?;
        let mut outcomes = Vec::with_capacity(pending.len());
        for stored in pending {
            let outcome = self.process_stored_job(stored).await?;
            outcomes.push(outcome);
            self.refresh_pending_best_effort().await;
        }
        Ok(outcomes)
    }

    async fn process_stored_job(
        &self,
        stored: StoredPrintJob,
    ) -> Result<ProcessOutcome, JobProcessingError> {
        let job = match serde_json::from_value::<PrintJob>(stored.payload.clone()) {
            Ok(job) => job,
            Err(error) => {
                return self
                    .persist_unreportable_failure(
                        &stored,
                        JobFailure::new("job.invalid_persisted", error.to_string(), false),
                    )
                    .await;
            }
        };
        if let Err(failure) = validate_persisted_job(&stored, &job) {
            return self.persist_failure(&stored, &job, failure).await;
        }

        let printer = match self.resolve_enabled_printer(&stored.printer_id).await {
            Ok(printer) => printer,
            Err(failure) => {
                return self.persist_failure(&stored, &job, failure).await;
            }
        };

        let rendered = match self.render(&job, &printer) {
            Ok(rendered) => rendered,
            Err(failure) => {
                return self.persist_failure(&stored, &job, failure).await;
            }
        };

        self.submit(&stored, &job, &printer, &rendered).await
    }

    async fn resolve_enabled_printer(
        &self,
        printer_id: &PrinterId,
    ) -> Result<PrinterRef, JobFailure> {
        let printer = self
            .printers
            .resolve(printer_id)
            .await
            .map_err(|error| JobFailure::new("printer.resolve_failed", error.to_string(), true))?
            .ok_or_else(|| {
                JobFailure::new(
                    "printer.not_found",
                    "the selected local printer is not configured",
                    true,
                )
            })?;
        if &printer.id != printer_id {
            return Err(JobFailure::new(
                "printer.identity_mismatch",
                "the printer resolver returned a different stable identity",
                false,
            ));
        }
        if !printer.enabled {
            return Err(JobFailure::new(
                "printer.disabled",
                "the selected local printer is disabled",
                true,
            ));
        }
        Ok(printer)
    }

    fn render(&self, job: &PrintJob, printer: &PrinterRef) -> Result<RenderedDocument, JobFailure> {
        let target = match &printer.connection {
            PrinterConnection::Virtual { .. } => RenderTarget::Virtual,
            PrinterConnection::SystemQueue { .. }
            | PrinterConnection::Network { .. }
            | PrinterConnection::Usb { .. } => RenderTarget::EscPos,
        };
        self.renderer
            .render(&job.document, target)
            .map_err(|error| renderer_failure(&error))
    }

    async fn submit(
        &self,
        stored: &StoredPrintJob,
        job: &PrintJob,
        printer: &PrinterRef,
        rendered: &RenderedDocument,
    ) -> Result<ProcessOutcome, JobProcessingError> {
        let cancellation = CancellationToken::new();
        {
            // Holding the map lock across the durable claim closes the gap in
            // which cancellation could otherwise miss an already-claimed job.
            let mut active = self.active.lock().await;
            match self.repository.mark_submitting(&stored.id).await {
                Ok(()) => {
                    active.insert(stored.id.clone(), cancellation.clone());
                }
                Err(StorageError::InvalidTransition { current, .. }) => {
                    return Ok(ProcessOutcome::Skipped {
                        job_id: stored.id.clone(),
                        state: current,
                    });
                }
                Err(error) => return Err(error.into()),
            }
        }
        self.handle.publish_job_event(AgentEvent::JobSubmitting {
            job_id: stored.id.clone(),
        });

        let submission = self
            .spoolers
            .submit(
                SubmissionRequest {
                    job_id: &stored.id,
                    printer,
                    document: rendered,
                },
                &cancellation,
            )
            .await;

        // Keep cancellation serialized with the terminal database write. Once
        // this map entry disappears, callers must observe a terminal row.
        let mut active = self.active.lock().await;
        let outcome: Result<ProcessOutcome, JobProcessingError> = async {
            match submission {
                Ok(receipt) => {
                    let message =
                        submitted_message(job, &stored.source_message_id, receipt.accepted_at)?;
                    let outbound = outbound_status(&stored.id, &message)?;
                    self.repository
                        .mark_submitted_with_status(&stored.id, receipt, &outbound)
                        .await?;
                    self.handle.publish_job_event(AgentEvent::JobSubmitted {
                        job_id: stored.id.clone(),
                    });
                    Ok(ProcessOutcome::Submitted {
                        job_id: stored.id.clone(),
                        message,
                    })
                }
                Err(error)
                    if cancellation.is_cancelled() || matches!(error, SpoolerError::Cancelled) =>
                {
                    self.repository.mark_cancelled(&stored.id).await?;
                    self.handle.publish_job_event(AgentEvent::JobCancelled {
                        job_id: stored.id.clone(),
                    });
                    Ok(ProcessOutcome::Cancelled {
                        job_id: stored.id.clone(),
                    })
                }
                Err(error) => {
                    let failure = spooler_failure(&error);
                    self.persist_failure_while_active(stored, job, failure)
                        .await
                }
            }
        }
        .await;
        active.remove(&stored.id);
        outcome
    }

    async fn persist_failure(
        &self,
        stored: &StoredPrintJob,
        job: &PrintJob,
        failure: JobFailure,
    ) -> Result<ProcessOutcome, JobProcessingError> {
        self.persist_failure_while_active(stored, job, failure)
            .await
    }

    async fn persist_failure_while_active(
        &self,
        stored: &StoredPrintJob,
        job: &PrintJob,
        failure: JobFailure,
    ) -> Result<ProcessOutcome, JobProcessingError> {
        let failure = recovery_aware_failure(stored, failure);
        let occurred_at = Timestamp::now();
        let message = failed_message(job, &stored.source_message_id, occurred_at, &failure)?;
        let outbound = outbound_status(&stored.id, &message)?;
        self.repository
            .mark_failed_with_status(&stored.id, failure.stored(occurred_at), &outbound)
            .await?;
        self.handle.publish_job_event(AgentEvent::JobFailed {
            job_id: stored.id.clone(),
            code: failure.code.to_owned(),
            retryable: failure.retryable,
        });
        Ok(ProcessOutcome::Failed {
            job_id: stored.id.clone(),
            message: Some(message),
        })
    }

    async fn persist_unreportable_failure(
        &self,
        stored: &StoredPrintJob,
        failure: JobFailure,
    ) -> Result<ProcessOutcome, JobProcessingError> {
        let failure = recovery_aware_failure(stored, failure);
        let occurred_at = Timestamp::now();
        self.repository
            .mark_failed(&stored.id, failure.stored(occurred_at))
            .await?;
        self.handle.publish_job_event(AgentEvent::JobFailed {
            job_id: stored.id.clone(),
            code: failure.code.to_owned(),
            retryable: failure.retryable,
        });
        Ok(ProcessOutcome::Failed {
            job_id: stored.id.clone(),
            message: None,
        })
    }

    async fn refresh_pending_best_effort(&self) {
        match self.repository.pending().await {
            Ok(pending) => self.handle.set_pending_jobs(pending.len()).await,
            Err(error) => {
                warn!(error = %error, "could not refresh pending job count");
            }
        }
    }
}

fn received_message(
    job: &PrintJob,
    correlation_id: &str,
    received_at: Timestamp,
) -> Result<AgentMessage, ValidationError> {
    agent_message(
        correlation_id,
        AgentMessageKind::JobReceived(JobReceived {
            job_id: job.job_id.clone(),
            idempotency_key: job.idempotency_key.clone(),
            status: JobStatus::Received,
            received_at: received_at.to_string(),
        }),
    )
}

fn received_message_for_stored(stored: &StoredPrintJob) -> Result<AgentMessage, ValidationError> {
    agent_message(
        &stored.source_message_id,
        AgentMessageKind::JobReceived(JobReceived {
            job_id: stored.id.to_string(),
            idempotency_key: stored.idempotency_key.clone(),
            status: JobStatus::Received,
            received_at: stored.received_at.to_string(),
        }),
    )
}

fn submitted_message(
    job: &PrintJob,
    correlation_id: &str,
    submitted_at: Timestamp,
) -> Result<AgentMessage, ValidationError> {
    agent_message(
        correlation_id,
        AgentMessageKind::JobSubmitted(JobSubmitted {
            job_id: job.job_id.clone(),
            idempotency_key: job.idempotency_key.clone(),
            printer_id: job.printer_id.clone(),
            status: JobStatus::Submitted,
            submitted_at: submitted_at.to_string(),
        }),
    )
}

fn failed_message(
    job: &PrintJob,
    correlation_id: &str,
    failed_at: Timestamp,
    failure: &JobFailure,
) -> Result<AgentMessage, ValidationError> {
    agent_message(
        correlation_id,
        AgentMessageKind::JobFailed(JobFailed {
            job_id: job.job_id.clone(),
            idempotency_key: job.idempotency_key.clone(),
            status: JobStatus::Failed,
            failed_at: failed_at.to_string(),
            error: FailureDetail {
                code: failure.code.to_owned(),
                message: failure.message.clone(),
                retryable: failure.retryable,
            },
        }),
    )
}

fn agent_message(
    correlation_id: &str,
    kind: AgentMessageKind,
) -> Result<AgentMessage, ValidationError> {
    let message = AgentMessage {
        protocol_version: ProtocolVersion::CURRENT,
        message_id: format!("msg_{}", Uuid::new_v4().simple()),
        sent_at: Timestamp::now().to_string(),
        correlation_id: Some(correlation_id.to_owned()),
        kind,
    };
    message.validate()?;
    Ok(message)
}

fn validate_persisted_job(stored: &StoredPrintJob, job: &PrintJob) -> Result<(), JobFailure> {
    job.validate()
        .map_err(|error| JobFailure::new("job.invalid_persisted", error.to_string(), false))?;
    if job.job_id != stored.id.as_str()
        || job.idempotency_key != stored.idempotency_key
        || job.printer_id != stored.printer_id.as_str()
    {
        return Err(JobFailure::new(
            "job.storage_mismatch",
            "persisted job identity does not match its durable row",
            false,
        ));
    }
    Ok(())
}

fn outbound_status(
    job_id: &PrintJobId,
    message: &AgentMessage,
) -> Result<NewOutboundStatus, serde_json::Error> {
    Ok(NewOutboundStatus {
        message_id: message.message_id.clone(),
        job_id: job_id.clone(),
        payload: serde_json::to_value(message)?,
        created_at: Timestamp::now(),
    })
}

fn renderer_failure(error: &RendererError) -> JobFailure {
    let retryable = matches!(error, RendererError::UnicodeRequiresRasterization);
    JobFailure::new("render.failed", error.to_string(), retryable)
}

fn recovery_aware_failure(stored: &StoredPrintJob, failure: JobFailure) -> JobFailure {
    if stored.recovery_count == 0 {
        return failure;
    }
    JobFailure::new(
        "job.recovery_uncertain",
        format!(
            "an interrupted submission may already have reached its backend; replay failed: {}",
            failure.message
        ),
        false,
    )
}

fn spooler_failure(error: &SpoolerError) -> JobFailure {
    let retryable = matches!(
        error,
        SpoolerError::BackendUnavailable(_)
            | SpoolerError::Connectivity(_)
            | SpoolerError::Timeout { .. }
            | SpoolerError::SimulatedFailure
    );
    JobFailure::new("spooler.failed", error.to_string(), retryable)
}

fn sanitize_message(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect();
    let sanitized = sanitized.trim();
    if sanitized.is_empty() {
        return "operation failed without diagnostic details".to_owned();
    }
    truncate_utf8(sanitized, 1_000).to_owned()
}

fn truncate_utf8(value: &str, maximum: usize) -> &str {
    if value.len() <= maximum {
        return value;
    }
    let mut boundary = maximum;
    while !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    &value[..boundary]
}
