//! Durable SQLite state for the OPPA agent.
//!
//! The repository enforces the safety-critical ordering boundary: a validated
//! job must be inserted successfully before the agent may acknowledge receipt.
//! Credentials are intentionally absent from this schema and belong in
//! `oppa-platform` secure storage.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::{
    path::Path,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use oppa_core::{JobState, PrintJobId, PrinterId, Timestamp};
use oppa_printer::{PrinterRef, SubmissionReceipt};
use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

const MIGRATIONS: &[(u32, &str)] = &[
    (1, include_str!("../migrations/0001_initial.sql")),
    (2, include_str!("../migrations/0002_job_indexes.sql")),
    (
        3,
        include_str!("../migrations/0003_job_retention_outbox.sql"),
    ),
];

/// Current SQLite schema version.
pub const CURRENT_MIGRATION_VERSION: u32 = 3;
/// Maximum serialized job payload accepted by the repository.
pub const MAX_JOB_PAYLOAD_BYTES: usize = 2 * 1024 * 1024;
/// Maximum serialized terminal status retained for transport replay.
pub const MAX_OUTBOUND_STATUS_BYTES: usize = 64 * 1024;
/// Default bound on non-terminal locally queued jobs.
pub const DEFAULT_MAX_PENDING_JOBS: usize = 10_000;
/// Default bound on retained terminal jobs and their pending status reports.
pub const DEFAULT_MAX_TERMINAL_JOBS: usize = 10_000;
/// Default page size for reading durable outbound statuses.
pub const DEFAULT_OUTBOUND_STATUS_BATCH_SIZE: usize = 256;

/// Explicit bounds for durable job storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageLimits {
    /// Maximum received or submitting rows.
    pub max_pending_jobs: usize,
    /// Maximum retained terminal rows, including unacknowledged statuses.
    pub max_terminal_jobs: usize,
}

impl Default for StorageLimits {
    fn default() -> Self {
        Self {
            max_pending_jobs: DEFAULT_MAX_PENDING_JOBS,
            max_terminal_jobs: DEFAULT_MAX_TERMINAL_JOBS,
        }
    }
}

/// Validated data persisted before acknowledging a delivered job.
///
/// `payload` is the already protocol-validated job representation. Keeping the
/// storage boundary protocol-neutral isolates schema evolution from SQL.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReceivedPrintJob {
    /// Protocol job identifier.
    pub id: PrintJobId,
    /// At-least-once delivery deduplication key.
    pub idempotency_key: String,
    /// Concrete stable local printer identity selected by the server.
    pub printer_id: PrinterId,
    /// Validated document/job payload required for restart recovery.
    pub payload: Value,
    /// Server message ID used to correlate receipt and terminal responses.
    pub source_message_id: String,
    /// Local receipt time.
    pub received_at: Timestamp,
}

/// A durable job restored from SQLite.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoredPrintJob {
    /// Job identifier.
    pub id: PrintJobId,
    /// Idempotency key.
    pub idempotency_key: String,
    /// Selected local printer.
    pub printer_id: PrinterId,
    /// Validated persisted payload.
    pub payload: Value,
    /// Original server message ID used for terminal response correlation.
    pub source_message_id: String,
    /// Durable lifecycle state.
    pub state: JobState,
    /// Initial local receipt time.
    pub received_at: Timestamp,
    /// Last state-change time.
    pub updated_at: Timestamp,
    /// Number of submission attempts claimed.
    pub retry_attempts: u32,
    /// Number of interrupted submissions recovered at startup.
    pub recovery_count: u32,
    /// Spooler receipt after successful submission.
    pub receipt: Option<SubmissionReceipt>,
    /// Sanitized final or recovery error.
    pub error: Option<StoredJobError>,
}

/// Sanitized failure information safe to keep in SQLite diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoredJobError {
    /// Stable machine-readable category.
    pub code: String,
    /// Bounded user-readable message without credentials or raw print data.
    pub message: String,
    /// Whether a future explicit retry may be appropriate.
    pub retryable: bool,
    /// Time the failure was persisted.
    pub occurred_at: Timestamp,
}

impl StoredJobError {
    /// Validates diagnostic bounds before persistence.
    pub fn validate(&self) -> StorageResult<()> {
        validate_key("job error code", &self.code, 64)?;
        if self.message.trim().is_empty() || self.message.len() > 2_000 {
            return Err(StorageError::InvalidInput(
                "job error message must be 1 to 2000 bytes".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Result of trying to durably insert an at-least-once delivery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertResult {
    /// A new row was committed and receipt may now be acknowledged.
    Inserted,
    /// The exact job ID and idempotency key were already persisted.
    DuplicateJob {
        /// Current durable lifecycle.
        state: JobState,
    },
    /// The idempotency key belongs to another already persisted job.
    DuplicateIdempotency {
        /// Existing job that owns the key.
        existing_job_id: PrintJobId,
        /// Existing durable lifecycle.
        state: JobState,
    },
}

/// Recovery summary returned during agent startup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryResult {
    /// Interrupted `submitting` rows moved back to `received`.
    pub recovered_submissions: usize,
    /// Total jobs now available for submission.
    pub pending_jobs: usize,
}

/// Protocol-neutral status payload committed atomically with a terminal job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewOutboundStatus {
    /// Stable protocol message identity used for idempotent replay.
    pub message_id: String,
    /// Terminal job associated with the report.
    pub job_id: PrintJobId,
    /// Already validated, bounded status message.
    pub payload: Value,
    /// Time at which the status entered the durable outbox.
    pub created_at: Timestamp,
}

/// Durable terminal status awaiting transport acknowledgement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoredOutboundStatus {
    /// Stable protocol message identity.
    pub message_id: String,
    /// Terminal job associated with the report.
    pub job_id: PrintJobId,
    /// Already validated, bounded status message.
    pub payload: Value,
    /// Time at which the status entered the durable outbox.
    pub created_at: Timestamp,
}

/// Result of terminal payload pruning and row-count retention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalRetentionResult {
    /// Terminal payloads replaced by `null` while retaining their fingerprints.
    pub payloads_pruned: usize,
    /// Oldest terminal rows deleted beyond the configured count.
    pub jobs_deleted: usize,
    /// Terminal rows retained after maintenance.
    pub jobs_retained: usize,
}

/// SQLite storage failures.
#[derive(Debug, Error)]
pub enum StorageError {
    /// A SQLite operation failed.
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// Structured data could not be encoded or decoded.
    #[error("stored JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
    /// A typed identifier persisted in SQLite was invalid.
    #[error("stored identifier is invalid: {0}")]
    Identifier(#[from] oppa_core::IdentifierError),
    /// Caller input was invalid.
    #[error("invalid storage input: {0}")]
    InvalidInput(String),
    /// The local pending queue reached its configured bound.
    #[error("local print queue is full (maximum {maximum} pending jobs)")]
    QueueFull {
        /// Configured non-terminal job limit.
        maximum: usize,
    },
    /// A reused job ID supplied a different idempotency key.
    #[error("job id {job_id} already exists with another idempotency key")]
    JobIdentityConflict {
        /// Conflicting identifier.
        job_id: PrintJobId,
    },
    /// A reused job ID and key changed the printer or validated job payload.
    #[error("job id {job_id} was redelivered with different printer or payload content")]
    JobContentConflict {
        /// Conflicting identifier.
        job_id: PrintJobId,
    },
    /// A lifecycle transition was invalid or raced another worker.
    #[error("cannot transition job {job_id} from {current:?} to {requested:?}")]
    InvalidTransition {
        /// Affected job.
        job_id: PrintJobId,
        /// Current durable state.
        current: JobState,
        /// Requested state.
        requested: JobState,
    },
    /// The requested durable job does not exist.
    #[error("job {0} was not found")]
    JobNotFound(PrintJobId),
    /// A blocking database worker failed.
    #[error("database worker failed: {0}")]
    Worker(String),
    /// A persisted timestamp could not be represented.
    #[error("stored timestamp {0} is outside the supported range")]
    Timestamp(i64),
}

/// Result alias for storage operations.
pub type StorageResult<T> = Result<T, StorageError>;

/// Async durable print-job repository used by the agent state machine.
#[async_trait]
pub trait JobRepository: Send + Sync {
    /// Commits a received job before any wire acknowledgement is sent.
    async fn insert_received(&self, job: &ReceivedPrintJob) -> StorageResult<InsertResult>;

    /// Lists jobs ready to be claimed, oldest first.
    async fn pending(&self) -> StorageResult<Vec<StoredPrintJob>>;

    /// Atomically claims a received job for one submission attempt.
    async fn mark_submitting(&self, id: &PrintJobId) -> StorageResult<()>;

    /// Persists successful backend acceptance.
    async fn mark_submitted(
        &self,
        id: &PrintJobId,
        receipt: SubmissionReceipt,
    ) -> StorageResult<()>;

    /// Atomically persists backend acceptance and its outbound status.
    async fn mark_submitted_with_status(
        &self,
        id: &PrintJobId,
        receipt: SubmissionReceipt,
        status: &NewOutboundStatus,
    ) -> StorageResult<()>;

    /// Persists a final submission failure.
    async fn mark_failed(&self, id: &PrintJobId, error: StoredJobError) -> StorageResult<()>;

    /// Atomically persists a final failure and its outbound status.
    async fn mark_failed_with_status(
        &self,
        id: &PrintJobId,
        error: StoredJobError,
        status: &NewOutboundStatus,
    ) -> StorageResult<()>;

    /// Marks an unsubmitted job cancelled.
    async fn mark_cancelled(&self, id: &PrintJobId) -> StorageResult<()>;

    /// Checks whether a terminal job already owns an idempotency key.
    async fn has_completed_idempotency_key(&self, key: &str) -> StorageResult<bool>;

    /// Restores interrupted submissions to the pending queue after restart.
    async fn recover_interrupted(&self) -> StorageResult<RecoveryResult>;

    /// Reads the oldest durable outbound terminal statuses, up to `limit`.
    async fn pending_outbound_statuses(
        &self,
        limit: usize,
    ) -> StorageResult<Vec<StoredOutboundStatus>>;

    /// Removes a status after its transport reports successful delivery.
    ///
    /// Returns whether a row was removed. Repeated acknowledgement is safe.
    async fn acknowledge_outbound_status(&self, message_id: &str) -> StorageResult<bool>;
}

/// Thread-safe SQLite implementation of OPPA durable state.
#[derive(Clone)]
pub struct SqliteStorage {
    connection: Arc<Mutex<Connection>>,
    max_pending_jobs: usize,
    max_terminal_jobs: usize,
}

impl SqliteStorage {
    /// Opens or creates a database and applies all embedded migrations.
    pub async fn open(path: impl AsRef<Path>, max_pending_jobs: usize) -> StorageResult<Self> {
        Self::open_with_limits(
            path,
            StorageLimits {
                max_pending_jobs,
                ..StorageLimits::default()
            },
        )
        .await
    }

    /// Opens or creates a database with explicit queue and retention bounds.
    pub async fn open_with_limits(
        path: impl AsRef<Path>,
        limits: StorageLimits,
    ) -> StorageResult<Self> {
        validate_limits(limits)?;
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|error| {
                StorageError::InvalidInput(format!(
                    "cannot create database directory {}: {error}",
                    parent.display()
                ))
            })?;
        }
        let connection = Connection::open(path)?;
        Self::from_connection(connection, limits).await
    }

    /// Creates a private in-memory database, primarily for tests.
    pub async fn in_memory() -> StorageResult<Self> {
        Self::in_memory_with_limits(StorageLimits::default()).await
    }

    /// Creates an in-memory database with explicit queue and retention bounds.
    pub async fn in_memory_with_limits(limits: StorageLimits) -> StorageResult<Self> {
        validate_limits(limits)?;
        Self::from_connection(Connection::open_in_memory()?, limits).await
    }

    async fn from_connection(connection: Connection, limits: StorageLimits) -> StorageResult<Self> {
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        let storage = Self {
            connection: Arc::new(Mutex::new(connection)),
            max_pending_jobs: limits.max_pending_jobs,
            max_terminal_jobs: limits.max_terminal_jobs,
        };
        storage.run(apply_migrations).await?;
        storage.maintain_terminal_jobs().await?;
        Ok(storage)
    }

    /// Returns the latest applied migration version.
    pub async fn migration_version(&self) -> StorageResult<u32> {
        self.run(|connection| {
            connection
                .query_row(
                    "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                    [],
                    |row| row.get(0),
                )
                .map_err(StorageError::from)
        })
        .await
    }

    /// Prunes terminal payload bodies and evicts the oldest rows over the
    /// configured retention count.
    pub async fn maintain_terminal_jobs(&self) -> StorageResult<TerminalRetentionResult> {
        let maximum = self.max_terminal_jobs;
        self.run(move |connection| {
            let transaction = connection.transaction()?;
            let result = enforce_terminal_retention(&transaction, maximum, None)?;
            transaction.commit()?;
            Ok(result)
        })
        .await
    }

    /// Upserts a configured printer reference.
    pub async fn save_printer(&self, printer: &PrinterRef) -> StorageResult<()> {
        printer
            .validate()
            .map_err(|error| StorageError::InvalidInput(error.to_string()))?;
        let printer_id = printer.id.to_string();
        let json = serde_json::to_string(printer)?;
        let now = now_millis()?;
        self.run(move |connection| {
            connection.execute(
                "INSERT INTO printers(printer_id, printer_json, updated_at_ms)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(printer_id) DO UPDATE SET
                    printer_json = excluded.printer_json,
                    updated_at_ms = excluded.updated_at_ms",
                params![printer_id, json, now],
            )?;
            Ok(())
        })
        .await
    }

    /// Lists configured printer references in stable ID order.
    pub async fn printers(&self) -> StorageResult<Vec<PrinterRef>> {
        self.run(|connection| {
            let mut statement =
                connection.prepare("SELECT printer_json FROM printers ORDER BY printer_id")?;
            let mut rows = statement.query([])?;
            let mut printers = Vec::new();
            while let Some(row) = rows.next()? {
                let json: String = row.get(0)?;
                printers.push(serde_json::from_str(&json)?);
            }
            Ok(printers)
        })
        .await
    }

    /// Stores a logical binding owned by the integrating application.
    pub async fn save_printer_binding(
        &self,
        binding_id: &str,
        printer_id: &PrinterId,
        metadata: &Value,
    ) -> StorageResult<()> {
        validate_key("binding id", binding_id, 256)?;
        let binding_id = binding_id.to_owned();
        let printer_id = printer_id.to_string();
        let metadata = bounded_json(metadata, 64 * 1024, "binding metadata")?;
        let now = now_millis()?;
        self.run(move |connection| {
            connection.execute(
                "INSERT INTO printer_bindings(binding_id, printer_id, metadata_json, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(binding_id) DO UPDATE SET
                   printer_id = excluded.printer_id,
                   metadata_json = excluded.metadata_json,
                   updated_at_ms = excluded.updated_at_ms",
                params![binding_id, printer_id, metadata, now],
            )?;
            Ok(())
        })
        .await
    }

    /// Stores bounded non-secret local settings JSON.
    pub async fn set_setting(&self, key: &str, value: &Value) -> StorageResult<()> {
        self.set_metadata("settings", key, value).await
    }

    /// Reads non-secret local settings JSON.
    pub async fn setting(&self, key: &str) -> StorageResult<Option<Value>> {
        self.get_metadata("settings", key).await
    }

    /// Stores bounded, non-secret connection metadata.
    pub async fn set_connection_metadata(&self, key: &str, value: &Value) -> StorageResult<()> {
        self.set_metadata("connection_metadata", key, value).await
    }

    /// Stores bounded sanitized diagnostics metadata.
    pub async fn set_diagnostics_metadata(&self, key: &str, value: &Value) -> StorageResult<()> {
        self.set_metadata("diagnostics_metadata", key, value).await
    }

    async fn set_metadata(
        &self,
        table: &'static str,
        key: &str,
        value: &Value,
    ) -> StorageResult<()> {
        validate_key("metadata key", key, 128)?;
        let key = key.to_owned();
        let value = bounded_json(value, 64 * 1024, "metadata")?;
        let now = now_millis()?;
        self.run(move |connection| {
            let sql = format!(
                "INSERT INTO {table}(key, value_json, updated_at_ms) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET
                   value_json = excluded.value_json,
                   updated_at_ms = excluded.updated_at_ms"
            );
            connection.execute(&sql, params![key, value, now])?;
            Ok(())
        })
        .await
    }

    async fn get_metadata(&self, table: &'static str, key: &str) -> StorageResult<Option<Value>> {
        validate_key("metadata key", key, 128)?;
        let key = key.to_owned();
        self.run(move |connection| {
            let sql = format!("SELECT value_json FROM {table} WHERE key = ?1");
            let json: Option<String> = connection
                .query_row(&sql, params![key], |row| row.get(0))
                .optional()?;
            json.map(|json| serde_json::from_str(&json))
                .transpose()
                .map_err(StorageError::from)
        })
        .await
    }

    async fn run<T, F>(&self, operation: F) -> StorageResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> StorageResult<T> + Send + 'static,
    {
        let connection = Arc::clone(&self.connection);
        tokio::task::spawn_blocking(move || operation(&mut connection.lock()))
            .await
            .map_err(|error| StorageError::Worker(error.to_string()))?
    }
}

#[async_trait]
impl JobRepository for SqliteStorage {
    async fn insert_received(&self, job: &ReceivedPrintJob) -> StorageResult<InsertResult> {
        validate_key("idempotency key", &job.idempotency_key, 256)?;
        validate_key("source message id", &job.source_message_id, 128)?;
        let payload = bounded_json(&job.payload, MAX_JOB_PAYLOAD_BYTES, "print job payload")?;
        let payload_sha256 = payload_fingerprint(&job.payload)?;
        let incoming_payload = job.payload.clone();
        let job_id = job.id.to_string();
        let typed_job_id = job.id.clone();
        let idempotency_key = job.idempotency_key.clone();
        let printer_id = job.printer_id.to_string();
        let source_message_id = job.source_message_id.clone();
        let received_at = job.received_at.as_datetime().timestamp_millis();
        let max_pending = self.max_pending_jobs;
        self.run(move |connection| {
            let transaction = connection.transaction()?;
            let by_id: Option<(String, String, String, String, String)> = transaction
                .query_row(
                    "SELECT idempotency_key, state, printer_id, payload_json, payload_sha256
                     FROM print_jobs WHERE job_id = ?1",
                    params![job_id],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                        ))
                    },
                )
                .optional()?;
            if let Some((
                existing_key,
                state,
                existing_printer,
                existing_payload,
                existing_payload_sha256,
            )) = by_id
            {
                if existing_key != idempotency_key {
                    return Err(StorageError::JobIdentityConflict {
                        job_id: typed_job_id,
                    });
                }
                let payload_matches = if existing_payload == "null" {
                    existing_payload_sha256 == payload_sha256
                } else {
                    serde_json::from_str::<Value>(&existing_payload)? == incoming_payload
                };
                if existing_printer != printer_id || !payload_matches {
                    return Err(StorageError::JobContentConflict {
                        job_id: typed_job_id,
                    });
                }
                return Ok(InsertResult::DuplicateJob {
                    state: parse_state(&state)?,
                });
            }
            let by_key: Option<(String, String)> = transaction
                .query_row(
                    "SELECT job_id, state FROM print_jobs WHERE idempotency_key = ?1",
                    params![idempotency_key],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            if let Some((existing_id, state)) = by_key {
                return Ok(InsertResult::DuplicateIdempotency {
                    existing_job_id: PrintJobId::new(existing_id)?,
                    state: parse_state(&state)?,
                });
            }
            let pending: i64 = transaction.query_row(
                "SELECT COUNT(*) FROM print_jobs WHERE state IN ('received', 'submitting')",
                [],
                |row| row.get(0),
            )?;
            let pending = usize_from_database(pending, "pending job count")?;
            if pending >= max_pending {
                return Err(StorageError::QueueFull {
                    maximum: max_pending,
                });
            }
            transaction.execute(
                "INSERT INTO print_jobs(
                    job_id, idempotency_key, printer_id, payload_json, state,
                    received_at_ms, updated_at_ms, source_message_id, payload_sha256
                 ) VALUES (?1, ?2, ?3, ?4, 'received', ?5, ?5, ?6, ?7)",
                params![
                    typed_job_id.to_string(),
                    idempotency_key,
                    printer_id,
                    payload,
                    received_at,
                    source_message_id,
                    payload_sha256
                ],
            )?;
            transaction.commit()?;
            Ok(InsertResult::Inserted)
        })
        .await
    }

    async fn pending(&self) -> StorageResult<Vec<StoredPrintJob>> {
        self.run(|connection| {
            load_jobs(
                connection,
                "SELECT job_id, idempotency_key, printer_id, payload_json, state,
                        received_at_ms, updated_at_ms, retry_attempts, recovery_count,
                        receipt_json, error_json, source_message_id
                 FROM print_jobs
                 WHERE state = 'received'
                 ORDER BY received_at_ms, job_id",
            )
        })
        .await
    }

    async fn mark_submitting(&self, id: &PrintJobId) -> StorageResult<()> {
        let id = id.clone();
        transition(self, id, JobState::Submitting, None, None, None).await
    }

    async fn mark_submitted(
        &self,
        id: &PrintJobId,
        receipt: SubmissionReceipt,
    ) -> StorageResult<()> {
        let receipt = serde_json::to_string(&receipt)?;
        transition(
            self,
            id.clone(),
            JobState::Submitted,
            Some(receipt),
            None,
            None,
        )
        .await
    }

    async fn mark_submitted_with_status(
        &self,
        id: &PrintJobId,
        receipt: SubmissionReceipt,
        status: &NewOutboundStatus,
    ) -> StorageResult<()> {
        let receipt = serde_json::to_string(&receipt)?;
        let status = prepare_outbound_status(id, status)?;
        transition(
            self,
            id.clone(),
            JobState::Submitted,
            Some(receipt),
            None,
            Some(status),
        )
        .await
    }

    async fn mark_failed(&self, id: &PrintJobId, error: StoredJobError) -> StorageResult<()> {
        error.validate()?;
        let error = serde_json::to_string(&error)?;
        transition(self, id.clone(), JobState::Failed, None, Some(error), None).await
    }

    async fn mark_failed_with_status(
        &self,
        id: &PrintJobId,
        error: StoredJobError,
        status: &NewOutboundStatus,
    ) -> StorageResult<()> {
        error.validate()?;
        let error = serde_json::to_string(&error)?;
        let status = prepare_outbound_status(id, status)?;
        transition(
            self,
            id.clone(),
            JobState::Failed,
            None,
            Some(error),
            Some(status),
        )
        .await
    }

    async fn mark_cancelled(&self, id: &PrintJobId) -> StorageResult<()> {
        transition(self, id.clone(), JobState::Cancelled, None, None, None).await
    }

    async fn has_completed_idempotency_key(&self, key: &str) -> StorageResult<bool> {
        validate_key("idempotency key", key, 256)?;
        let key = key.to_owned();
        self.run(move |connection| {
            connection
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM print_jobs
                       WHERE idempotency_key = ?1
                         AND state IN ('submitted', 'failed', 'cancelled')
                     )",
                    params![key],
                    |row| row.get(0),
                )
                .map_err(StorageError::from)
        })
        .await
    }

    async fn recover_interrupted(&self) -> StorageResult<RecoveryResult> {
        let now = now_millis()?;
        self.run(move |connection| {
            let recovery_error = serde_json::to_string(&StoredJobError {
                code: "agent-restarted".to_owned(),
                message: "submission was interrupted by agent restart; outcome is unknown"
                    .to_owned(),
                retryable: true,
                occurred_at: timestamp_from_millis(now)?,
            })?;
            let recovered = connection.execute(
                "UPDATE print_jobs
                 SET state = 'received',
                     recovery_count = recovery_count + 1,
                     updated_at_ms = ?1,
                     error_json = ?2
                 WHERE state = 'submitting'",
                params![now, recovery_error],
            )?;
            let pending_jobs: i64 = connection.query_row(
                "SELECT COUNT(*) FROM print_jobs WHERE state = 'received'",
                [],
                |row| row.get(0),
            )?;
            Ok(RecoveryResult {
                recovered_submissions: recovered,
                pending_jobs: usize_from_database(pending_jobs, "pending job count")?,
            })
        })
        .await
    }

    async fn pending_outbound_statuses(
        &self,
        limit: usize,
    ) -> StorageResult<Vec<StoredOutboundStatus>> {
        if limit == 0 {
            return Err(StorageError::InvalidInput(
                "outbound status read limit must be greater than zero".to_owned(),
            ));
        }
        let limit = i64::try_from(limit).map_err(|_| {
            StorageError::InvalidInput("outbound status read limit is too large".to_owned())
        })?;
        self.run(move |connection| {
            let mut statement = connection.prepare(
                "SELECT message_id, job_id, payload_json, created_at_ms
                 FROM outbound_status_outbox
                 ORDER BY created_at_ms, message_id
                 LIMIT ?1",
            )?;
            let mut rows = statement.query(params![limit])?;
            let mut statuses = Vec::new();
            while let Some(row) = rows.next()? {
                statuses.push(StoredOutboundStatus {
                    message_id: row.get(0)?,
                    job_id: PrintJobId::new(row.get::<_, String>(1)?)?,
                    payload: serde_json::from_str(&row.get::<_, String>(2)?)?,
                    created_at: timestamp_from_millis(row.get(3)?)?,
                });
            }
            Ok(statuses)
        })
        .await
    }

    async fn acknowledge_outbound_status(&self, message_id: &str) -> StorageResult<bool> {
        validate_key("outbound message id", message_id, 128)?;
        let message_id = message_id.to_owned();
        self.run(move |connection| {
            Ok(connection.execute(
                "DELETE FROM outbound_status_outbox WHERE message_id = ?1",
                params![message_id],
            )? == 1)
        })
        .await
    }
}

#[derive(Debug)]
struct PreparedOutboundStatus {
    message_id: String,
    job_id: String,
    payload_json: String,
    created_at_ms: i64,
}

async fn transition(
    storage: &SqliteStorage,
    id: PrintJobId,
    requested: JobState,
    receipt_json: Option<String>,
    error_json: Option<String>,
    outbound_status: Option<PreparedOutboundStatus>,
) -> StorageResult<()> {
    let now = now_millis()?;
    let max_terminal_jobs = storage.max_terminal_jobs;
    storage
        .run(move |connection| {
            let transaction = connection.transaction()?;
            let state: Option<String> = transaction
                .query_row(
                    "SELECT state FROM print_jobs WHERE job_id = ?1",
                    params![id.to_string()],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(state) = state else {
                return Err(StorageError::JobNotFound(id));
            };
            let current = parse_state(&state)?;
            let allowed = match requested {
                JobState::Submitting => current == JobState::Received,
                JobState::Submitted => current == JobState::Submitting,
                JobState::Failed => {
                    matches!(current, JobState::Received | JobState::Submitting)
                }
                JobState::Cancelled => {
                    matches!(current, JobState::Received | JobState::Submitting)
                }
                JobState::Received => false,
            };
            if !allowed {
                return Err(StorageError::InvalidTransition {
                    job_id: id,
                    current,
                    requested,
                });
            }
            let state = state_name(requested);
            let attempt_increment = if requested == JobState::Submitting {
                1_i64
            } else {
                0_i64
            };
            transaction.execute(
                "UPDATE print_jobs
                 SET state = ?1,
                     updated_at_ms = ?2,
                     retry_attempts = retry_attempts + ?3,
                     receipt_json = COALESCE(?4, receipt_json),
                     error_json = ?5
                 WHERE job_id = ?6",
                params![
                    state,
                    now,
                    attempt_increment,
                    receipt_json,
                    error_json,
                    id.to_string()
                ],
            )?;
            if let Some(status) = outbound_status {
                transaction.execute(
                    "INSERT INTO outbound_status_outbox(
                        message_id, job_id, payload_json, created_at_ms
                     ) VALUES (?1, ?2, ?3, ?4)",
                    params![
                        status.message_id,
                        status.job_id,
                        status.payload_json,
                        status.created_at_ms
                    ],
                )?;
            }
            if requested.is_terminal() {
                enforce_terminal_retention(&transaction, max_terminal_jobs, Some(id.as_str()))?;
            }
            transaction.commit()?;
            Ok(())
        })
        .await
}

fn apply_migrations(connection: &mut Connection) -> StorageResult<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY NOT NULL,
            applied_at_ms INTEGER NOT NULL
         );",
    )?;
    let current: u32 = connection.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;
    if current > CURRENT_MIGRATION_VERSION {
        return Err(StorageError::InvalidInput(format!(
            "database schema {current} is newer than supported schema {CURRENT_MIGRATION_VERSION}"
        )));
    }
    for (version, sql) in MIGRATIONS
        .iter()
        .copied()
        .filter(|(version, _)| *version > current)
    {
        let transaction = connection.transaction()?;
        transaction.execute_batch(sql)?;
        if version == 3 {
            backfill_payload_fingerprints(&transaction)?;
        }
        transaction.execute(
            "INSERT INTO schema_migrations(version, applied_at_ms) VALUES (?1, ?2)",
            params![version, now_millis()?],
        )?;
        transaction.commit()?;
    }
    Ok(())
}

fn load_jobs(connection: &Connection, sql: &str) -> StorageResult<Vec<StoredPrintJob>> {
    let mut statement = connection.prepare(sql)?;
    let mut rows = statement.query([])?;
    let mut jobs = Vec::new();
    while let Some(row) = rows.next()? {
        let payload_json: String = row.get(3)?;
        let receipt_json: Option<String> = row.get(9)?;
        let error_json: Option<String> = row.get(10)?;
        jobs.push(StoredPrintJob {
            id: PrintJobId::new(row.get::<_, String>(0)?)?,
            idempotency_key: row.get(1)?,
            printer_id: PrinterId::new(row.get::<_, String>(2)?)?,
            payload: serde_json::from_str(&payload_json)?,
            state: parse_state(&row.get::<_, String>(4)?)?,
            received_at: timestamp_from_millis(row.get(5)?)?,
            updated_at: timestamp_from_millis(row.get(6)?)?,
            retry_attempts: row.get(7)?,
            recovery_count: row.get(8)?,
            receipt: receipt_json
                .map(|json| serde_json::from_str(&json))
                .transpose()?,
            error: error_json
                .map(|json| serde_json::from_str(&json))
                .transpose()?,
            source_message_id: row.get(11)?,
        });
    }
    Ok(jobs)
}

fn parse_state(value: &str) -> StorageResult<JobState> {
    match value {
        "received" => Ok(JobState::Received),
        "submitting" => Ok(JobState::Submitting),
        "submitted" => Ok(JobState::Submitted),
        "failed" => Ok(JobState::Failed),
        "cancelled" => Ok(JobState::Cancelled),
        other => Err(StorageError::InvalidInput(format!(
            "database contains unknown job state {other:?}"
        ))),
    }
}

const fn state_name(state: JobState) -> &'static str {
    match state {
        JobState::Received => "received",
        JobState::Submitting => "submitting",
        JobState::Submitted => "submitted",
        JobState::Failed => "failed",
        JobState::Cancelled => "cancelled",
    }
}

fn prepare_outbound_status(
    job_id: &PrintJobId,
    status: &NewOutboundStatus,
) -> StorageResult<PreparedOutboundStatus> {
    validate_key("outbound message id", &status.message_id, 128)?;
    if &status.job_id != job_id {
        return Err(StorageError::InvalidInput(
            "outbound status job id must match its terminal transition".to_owned(),
        ));
    }
    Ok(PreparedOutboundStatus {
        message_id: status.message_id.clone(),
        job_id: status.job_id.to_string(),
        payload_json: bounded_json(
            &status.payload,
            MAX_OUTBOUND_STATUS_BYTES,
            "outbound status payload",
        )?,
        created_at_ms: status.created_at.as_datetime().timestamp_millis(),
    })
}

fn enforce_terminal_retention(
    connection: &Connection,
    maximum: usize,
    protected_job_id: Option<&str>,
) -> StorageResult<TerminalRetentionResult> {
    let payloads_pruned = connection.execute(
        "UPDATE print_jobs
         SET payload_json = 'null'
         WHERE state IN ('submitted', 'failed', 'cancelled')
           AND payload_json <> 'null'",
        [],
    )?;
    let terminal_jobs: i64 = connection.query_row(
        "SELECT COUNT(*) FROM print_jobs
         WHERE state IN ('submitted', 'failed', 'cancelled')",
        [],
        |row| row.get(0),
    )?;
    let terminal_jobs = usize_from_database(terminal_jobs, "terminal job count")?;
    let excess = terminal_jobs.saturating_sub(maximum);
    let jobs_deleted = if excess == 0 {
        0
    } else {
        let excess = i64::try_from(excess).map_err(|_| {
            StorageError::InvalidInput("terminal retention excess is too large".to_owned())
        })?;
        connection.execute(
            "DELETE FROM print_jobs
             WHERE job_id IN (
                 SELECT job_id FROM print_jobs
                 WHERE state IN ('submitted', 'failed', 'cancelled')
                   AND (?1 IS NULL OR job_id <> ?1)
                 ORDER BY updated_at_ms, job_id
                 LIMIT ?2
             )",
            params![protected_job_id, excess],
        )?
    };
    let jobs_retained: i64 = connection.query_row(
        "SELECT COUNT(*) FROM print_jobs
         WHERE state IN ('submitted', 'failed', 'cancelled')",
        [],
        |row| row.get(0),
    )?;
    Ok(TerminalRetentionResult {
        payloads_pruned,
        jobs_deleted,
        jobs_retained: usize_from_database(jobs_retained, "retained terminal job count")?,
    })
}

fn backfill_payload_fingerprints(connection: &Connection) -> StorageResult<()> {
    let rows = {
        let mut statement = connection
            .prepare("SELECT job_id, payload_json FROM print_jobs WHERE payload_sha256 = ''")?;
        let mut rows = statement.query([])?;
        let mut payloads = Vec::new();
        while let Some(row) = rows.next()? {
            payloads.push((row.get::<_, String>(0)?, row.get::<_, String>(1)?));
        }
        payloads
    };
    for (job_id, payload_json) in rows {
        let payload: Value = serde_json::from_str(&payload_json)?;
        connection.execute(
            "UPDATE print_jobs SET payload_sha256 = ?1 WHERE job_id = ?2",
            params![payload_fingerprint(&payload)?, job_id],
        )?;
    }
    Ok(())
}

fn payload_fingerprint(payload: &Value) -> StorageResult<String> {
    let canonical = serde_json::to_vec(payload)?;
    Ok(format!("{:x}", Sha256::digest(canonical)))
}

fn validate_limits(limits: StorageLimits) -> StorageResult<()> {
    if limits.max_pending_jobs == 0 || limits.max_terminal_jobs == 0 {
        return Err(StorageError::InvalidInput(
            "pending and terminal job limits must be greater than zero".to_owned(),
        ));
    }
    Ok(())
}

fn bounded_json(value: &Value, maximum: usize, field: &str) -> StorageResult<String> {
    let json = serde_json::to_string(value)?;
    if json.len() > maximum {
        return Err(StorageError::InvalidInput(format!(
            "{field} is {} bytes; maximum is {maximum}",
            json.len()
        )));
    }
    Ok(json)
}

fn validate_key(field: &str, value: &str, maximum: usize) -> StorageResult<()> {
    if value.trim().is_empty()
        || value.trim() != value
        || value.len() > maximum
        || value.chars().any(char::is_control)
    {
        return Err(StorageError::InvalidInput(format!(
            "{field} must be 1 to {maximum} bytes without surrounding whitespace or control characters"
        )));
    }
    Ok(())
}

fn now_millis() -> StorageResult<i64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            StorageError::InvalidInput(format!("system clock is before epoch: {error}"))
        })?;
    i64::try_from(duration.as_millis())
        .map_err(|_| StorageError::InvalidInput("system time is too large".to_owned()))
}

fn timestamp_from_millis(value: i64) -> StorageResult<Timestamp> {
    DateTime::<Utc>::from_timestamp_millis(value)
        .map(Timestamp::from_datetime)
        .ok_or(StorageError::Timestamp(value))
}

fn usize_from_database(value: i64, field: &str) -> StorageResult<usize> {
    usize::try_from(value).map_err(|_| {
        StorageError::InvalidInput(format!(
            "database {field} {value} is negative or exceeds this platform"
        ))
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn received(id: &str, key: &str) -> ReceivedPrintJob {
        ReceivedPrintJob {
            id: PrintJobId::new(id).expect("fixture id"),
            idempotency_key: key.to_owned(),
            printer_id: PrinterId::new("printer_1").expect("fixture printer"),
            payload: serde_json::json!({"document": {"width": 80, "sections": []}}),
            source_message_id: format!("server_{id}"),
            received_at: Timestamp::now(),
        }
    }

    fn receipt() -> SubmissionReceipt {
        SubmissionReceipt {
            backend_job_id: Some("virtual_1".to_owned()),
            backend: "virtual".to_owned(),
            accepted_at: Timestamp::now(),
            metadata: BTreeMap::new(),
        }
    }

    fn stored_error() -> StoredJobError {
        StoredJobError {
            code: "spooler.failed".to_owned(),
            message: "virtual backend rejected submission".to_owned(),
            retryable: true,
            occurred_at: Timestamp::now(),
        }
    }

    fn outbound_status(job_id: &str, message_id: &str) -> NewOutboundStatus {
        NewOutboundStatus {
            message_id: message_id.to_owned(),
            job_id: PrintJobId::new(job_id).expect("fixture job"),
            payload: serde_json::json!({
                "type": "agent.job_failed",
                "payload": {"jobId": job_id}
            }),
            created_at: Timestamp::now(),
        }
    }

    #[tokio::test]
    async fn migrations_and_insertion_are_ready_before_acknowledgement() {
        let storage = SqliteStorage::in_memory().await.expect("storage");
        assert_eq!(
            storage.migration_version().await.expect("version"),
            CURRENT_MIGRATION_VERSION
        );
        assert_eq!(
            storage
                .insert_received(&received("job_1", "invoice_1"))
                .await
                .expect("insert"),
            InsertResult::Inserted
        );
        let pending = storage.pending().await.expect("pending");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].state, JobState::Received);
    }

    #[tokio::test]
    async fn duplicate_job_and_idempotency_key_are_distinguished() {
        let storage = SqliteStorage::in_memory().await.expect("storage");
        let job = received("job_1", "invoice_1");
        storage.insert_received(&job).await.expect("initial insert");
        assert_eq!(
            storage.insert_received(&job).await.expect("duplicate"),
            InsertResult::DuplicateJob {
                state: JobState::Received
            }
        );
        assert_eq!(
            storage
                .insert_received(&received("job_2", "invoice_1"))
                .await
                .expect("idempotent duplicate"),
            InsertResult::DuplicateIdempotency {
                existing_job_id: PrintJobId::new("job_1").expect("fixture"),
                state: JobState::Received,
            }
        );
        assert!(matches!(
            storage
                .insert_received(&received("job_1", "other-key"))
                .await,
            Err(StorageError::JobIdentityConflict { .. })
        ));

        let mut changed_printer = job.clone();
        changed_printer.printer_id = PrinterId::new("printer_2").expect("fixture printer");
        assert!(matches!(
            storage.insert_received(&changed_printer).await,
            Err(StorageError::JobContentConflict { .. })
        ));

        let mut changed_payload = job;
        changed_payload.payload = serde_json::json!({"document": {"width": 58, "sections": []}});
        assert!(matches!(
            storage.insert_received(&changed_payload).await,
            Err(StorageError::JobContentConflict { .. })
        ));
    }

    #[tokio::test]
    async fn pruned_terminal_payload_still_detects_exact_and_changed_redelivery() {
        let storage = SqliteStorage::in_memory().await.expect("storage");
        let job = received("job_1", "invoice_1");
        let id = job.id.clone();
        storage.insert_received(&job).await.expect("insert");
        storage
            .mark_failed(&id, stored_error())
            .await
            .expect("terminal transition");

        assert_eq!(
            storage
                .insert_received(&job)
                .await
                .expect("exact duplicate"),
            InsertResult::DuplicateJob {
                state: JobState::Failed
            }
        );
        let mut changed = job;
        changed.payload = serde_json::json!({"document": {"width": 58, "sections": []}});
        assert!(matches!(
            storage.insert_received(&changed).await,
            Err(StorageError::JobContentConflict { .. })
        ));

        let payload: String = storage
            .run(|connection| {
                connection
                    .query_row(
                        "SELECT payload_json FROM print_jobs WHERE job_id = 'job_1'",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(StorageError::from)
            })
            .await
            .expect("stored payload");
        assert_eq!(payload, "null");
    }

    #[tokio::test]
    async fn state_machine_rejects_skipped_or_repeated_submission() {
        let storage = SqliteStorage::in_memory().await.expect("storage");
        let id = PrintJobId::new("job_1").expect("fixture");
        storage
            .insert_received(&received("job_1", "invoice_1"))
            .await
            .expect("insert");
        assert!(matches!(
            storage.mark_submitted(&id, receipt()).await,
            Err(StorageError::InvalidTransition { .. })
        ));
        storage.mark_submitting(&id).await.expect("claim");
        storage
            .mark_submitted(&id, receipt())
            .await
            .expect("complete");
        assert!(
            storage
                .has_completed_idempotency_key("invoice_1")
                .await
                .expect("idempotency query")
        );
        assert!(matches!(
            storage.mark_submitting(&id).await,
            Err(StorageError::InvalidTransition { .. })
        ));
    }

    #[tokio::test]
    async fn restart_recovery_requeues_interrupted_submission_on_disk() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("agent.sqlite");
        let id = PrintJobId::new("job_1").expect("fixture");
        {
            let storage = SqliteStorage::open(&path, 10).await.expect("open");
            storage
                .insert_received(&received("job_1", "invoice_1"))
                .await
                .expect("insert");
            storage.mark_submitting(&id).await.expect("claim");
        }
        let reopened = SqliteStorage::open(&path, 10).await.expect("reopen");
        let recovery = reopened
            .recover_interrupted()
            .await
            .expect("recover interrupted");
        assert_eq!(recovery.recovered_submissions, 1);
        let pending = reopened.pending().await.expect("pending");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].recovery_count, 1);
        assert_eq!(pending[0].retry_attempts, 1);
        assert_eq!(
            pending[0].error.as_ref().map(|error| error.code.as_str()),
            Some("agent-restarted")
        );
    }

    #[tokio::test]
    async fn pending_queue_is_bounded() {
        let storage = SqliteStorage::from_connection(
            Connection::open_in_memory().expect("sqlite"),
            StorageLimits {
                max_pending_jobs: 1,
                ..StorageLimits::default()
            },
        )
        .await
        .expect("storage");
        storage
            .insert_received(&received("job_1", "key_1"))
            .await
            .expect("first");
        assert!(matches!(
            storage.insert_received(&received("job_2", "key_2")).await,
            Err(StorageError::QueueFull { maximum: 1 })
        ));
    }

    #[tokio::test]
    async fn terminal_rows_and_payloads_are_bounded_automatically() {
        let storage = SqliteStorage::in_memory_with_limits(StorageLimits {
            max_pending_jobs: 10,
            max_terminal_jobs: 2,
        })
        .await
        .expect("storage");

        for index in 1..=3 {
            let job_id = format!("job_{index}");
            let job = received(&job_id, &format!("key_{index}"));
            storage.insert_received(&job).await.expect("insert");
            storage
                .mark_failed_with_status(
                    &job.id,
                    stored_error(),
                    &outbound_status(&job_id, &format!("agent_message_{index}")),
                )
                .await
                .expect("terminal transition");
        }

        let (terminal_jobs, retained_payloads, outbound_statuses): (i64, i64, i64) = storage
            .run(|connection| {
                Ok((
                    connection.query_row(
                        "SELECT COUNT(*) FROM print_jobs
                         WHERE state IN ('submitted', 'failed', 'cancelled')",
                        [],
                        |row| row.get(0),
                    )?,
                    connection.query_row(
                        "SELECT COUNT(*) FROM print_jobs
                         WHERE state IN ('submitted', 'failed', 'cancelled')
                           AND payload_json <> 'null'",
                        [],
                        |row| row.get(0),
                    )?,
                    connection.query_row(
                        "SELECT COUNT(*) FROM outbound_status_outbox",
                        [],
                        |row| row.get(0),
                    )?,
                ))
            })
            .await
            .expect("retention counts");
        assert_eq!(terminal_jobs, 2);
        assert_eq!(retained_payloads, 0);
        assert_eq!(outbound_statuses, 2);
        assert!(
            !storage
                .has_completed_idempotency_key("key_1")
                .await
                .expect("evicted key")
        );
        assert!(
            storage
                .has_completed_idempotency_key("key_3")
                .await
                .expect("retained key")
        );
    }

    #[tokio::test]
    async fn terminal_status_outbox_survives_restart_and_acknowledges_idempotently() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("agent.sqlite");
        {
            let storage = SqliteStorage::open(&path, 10).await.expect("open");
            let job = received("job_1", "key_1");
            storage.insert_received(&job).await.expect("insert");
            storage
                .mark_failed_with_status(
                    &job.id,
                    stored_error(),
                    &outbound_status("job_1", "agent_message_1"),
                )
                .await
                .expect("atomic terminal status");
        }

        let reopened = SqliteStorage::open(&path, 10).await.expect("reopen");
        let statuses = reopened
            .pending_outbound_statuses(DEFAULT_OUTBOUND_STATUS_BATCH_SIZE)
            .await
            .expect("pending statuses");
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].message_id, "agent_message_1");
        assert_eq!(statuses[0].job_id.as_str(), "job_1");
        assert!(
            reopened
                .acknowledge_outbound_status("agent_message_1")
                .await
                .expect("first acknowledgement")
        );
        assert!(
            !reopened
                .acknowledge_outbound_status("agent_message_1")
                .await
                .expect("repeated acknowledgement")
        );
        assert!(
            reopened
                .pending_outbound_statuses(DEFAULT_OUTBOUND_STATUS_BATCH_SIZE)
                .await
                .expect("empty outbox")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn outbox_validation_rolls_back_the_terminal_transition() {
        let storage = SqliteStorage::in_memory().await.expect("storage");
        let job = received("job_1", "key_1");
        storage.insert_received(&job).await.expect("insert");
        let mismatched = outbound_status("job_2", "agent_message_1");
        assert!(matches!(
            storage
                .mark_failed_with_status(&job.id, stored_error(), &mismatched)
                .await,
            Err(StorageError::InvalidInput(_))
        ));
        let pending = storage.pending().await.expect("pending");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].state, JobState::Received);
    }
}
