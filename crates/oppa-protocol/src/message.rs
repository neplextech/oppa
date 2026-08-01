use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::{
    MAX_PRINTERS_PER_INVENTORY, MAX_WIRE_MESSAGE_BYTES, PROTOCOL_VERSION, PrintJob,
    PrinterDescriptor, Validate, ValidationError,
    validation::{validate_brand_name, validate_identifier, validate_string, validate_timestamp},
};

const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

/// Validated OpenPrinter protocol version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProtocolVersion;

impl ProtocolVersion {
    /// Current protocol version used by new messages.
    pub const CURRENT: Self = Self;

    /// Returns the stable string wire representation.
    pub const fn get(self) -> &'static str {
        PROTOCOL_VERSION
    }
}

impl Default for ProtocolVersion {
    fn default() -> Self {
        Self::CURRENT
    }
}

impl Serialize for ProtocolVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(PROTOCOL_VERSION)
    }
}

impl<'de> Deserialize<'de> for ProtocolVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct VersionVisitor;

        impl de::Visitor<'_> for VersionVisitor {
            type Value = ProtocolVersion;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, "protocol version {PROTOCOL_VERSION}")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if value == PROTOCOL_VERSION {
                    Ok(ProtocolVersion)
                } else {
                    Err(E::custom(format!("unsupported protocol version {value}")))
                }
            }
        }

        deserializer.deserialize_any(VersionVisitor)
    }
}

/// All lifecycle terms reserved by OpenPrinter v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    /// Waiting in a durable host-side queue.
    Queued,
    /// Sent across a transport but not yet durably acknowledged.
    Delivered,
    /// Durably persisted by the agent.
    Received,
    /// Accepted by a printer backend.
    Submitted,
    /// Failed before or during backend submission.
    Failed,
    /// Cancelled before backend submission.
    Cancelled,
}

/// Initial agent handshake and version advertisement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentHello {
    /// Stable installation identity.
    pub agent_id: String,
    /// Running agent build version.
    pub agent_version: String,
    /// Compile-time product identity.
    pub product_id: String,
    /// Compile-time product version.
    pub product_version: String,
    /// Protocol versions understood by this agent.
    pub supported_protocol_versions: Vec<ProtocolVersion>,
}

impl Validate for AgentHello {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_identifier(&self.agent_id, "agentId", 128)?;
        validate_string(&self.agent_version, "agentVersion", 1, 256)?;
        validate_identifier(&self.product_id, "productId", 128)?;
        validate_string(&self.product_version, "productVersion", 1, 256)?;
        validate_versions(
            &self.supported_protocol_versions,
            "supportedProtocolVersions",
        )
    }
}

/// Correlated response to a server heartbeat request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentHeartbeat {
    /// Whole seconds since the current agent process started.
    pub uptime_seconds: u64,
}

impl Validate for AgentHeartbeat {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_safe_integer(self.uptime_seconds, "uptimeSeconds")
    }
}

/// Complete printer inventory, either periodic or request-correlated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrinterInventory {
    /// Monotonically increasing agent-local inventory revision.
    pub revision: u64,
    /// Complete printers visible at this revision.
    pub printers: Vec<PrinterDescriptor>,
}

impl Validate for PrinterInventory {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_safe_integer(self.revision, "revision")?;
        validate_printers(&self.printers, "printers")
    }
}

/// Incremental printer inventory update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrinterInventoryChanged {
    /// Monotonically increasing agent-local inventory revision.
    pub revision: u64,
    /// Newly discovered printers.
    pub added: Vec<PrinterDescriptor>,
    /// Descriptors changed since the prior revision.
    pub updated: Vec<PrinterDescriptor>,
    /// Stable IDs no longer present.
    pub removed_printer_ids: Vec<String>,
}

impl Validate for PrinterInventoryChanged {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_safe_integer(self.revision, "revision")?;
        validate_printers(&self.added, "added")?;
        validate_printers(&self.updated, "updated")?;
        if self.removed_printer_ids.len() > MAX_PRINTERS_PER_INVENTORY {
            return Err(ValidationError::new(
                "removedPrinterIds",
                format!("must contain at most {MAX_PRINTERS_PER_INVENTORY} identifiers"),
            ));
        }
        for (index, printer_id) in self.removed_printer_ids.iter().enumerate() {
            validate_identifier(printer_id, format!("removedPrinterIds.{index}"), 128)?;
            if self.removed_printer_ids[..index].contains(printer_id) {
                return Err(ValidationError::new(
                    format!("removedPrinterIds.{index}"),
                    "must not contain duplicate identifiers",
                ));
            }
        }
        Ok(())
    }
}

/// Durable-persistence acknowledgement for a delivered job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobReceived {
    /// Durable job identity.
    pub job_id: String,
    /// Duplicate-detection key copied from the job.
    pub idempotency_key: String,
    /// Literal `received` lifecycle state.
    pub status: JobStatus,
    /// UTC time at which durable local persistence completed.
    pub received_at: String,
}

impl Validate for JobReceived {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_job_identity(&self.job_id, &self.idempotency_key)?;
        validate_expected_status(self.status, JobStatus::Received)?;
        validate_timestamp(&self.received_at, "receivedAt")
    }
}

/// Backend-acceptance result that does not imply physical output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobSubmitted {
    /// Durable job identity.
    pub job_id: String,
    /// Duplicate-detection key copied from the job.
    pub idempotency_key: String,
    /// Concrete printer that accepted submission.
    pub printer_id: String,
    /// Literal `submitted` lifecycle state.
    pub status: JobStatus,
    /// UTC time at which backend submission completed.
    pub submitted_at: String,
}

impl Validate for JobSubmitted {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_job_identity(&self.job_id, &self.idempotency_key)?;
        validate_identifier(&self.printer_id, "printerId", 128)?;
        validate_expected_status(self.status, JobStatus::Submitted)?;
        validate_timestamp(&self.submitted_at, "submittedAt")
    }
}

/// Bounded failure information safe to transmit and diagnose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FailureDetail {
    /// Stable machine-readable error category.
    pub code: String,
    /// Bounded human-readable explanation without credentials or payload data.
    pub message: String,
    /// Whether policy permits retrying the same idempotent job.
    pub retryable: bool,
}

impl Validate for FailureDetail {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_identifier(&self.code, "code", 128)?;
        validate_string(&self.message, "message", 1, 1_024)
    }
}

/// Recoverable or terminal failure after job delivery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobFailed {
    /// Durable job identity.
    pub job_id: String,
    /// Duplicate-detection key copied from the job.
    pub idempotency_key: String,
    /// Literal `failed` lifecycle state.
    pub status: JobStatus,
    /// UTC time at which the failure became known.
    pub failed_at: String,
    /// Sanitized failure details.
    pub error: FailureDetail,
}

impl Validate for JobFailed {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_job_identity(&self.job_id, &self.idempotency_key)?;
        validate_expected_status(self.status, JobStatus::Failed)?;
        validate_timestamp(&self.failed_at, "failedAt")?;
        self.error.validate().map_err(|error| error.at("error"))
    }
}

/// Coarse agent health state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentHealth {
    /// No known operational issue.
    Healthy,
    /// Operational with one or more degraded capabilities.
    Degraded,
    /// Unable to perform core duties.
    Unhealthy,
}

/// Diagnostic issue severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    /// Informational condition.
    Info,
    /// Degraded condition that may need attention.
    Warning,
    /// Condition preventing expected operation.
    Error,
}

/// Sanitized diagnostic issue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiagnosticIssue {
    /// Stable machine-readable condition.
    pub code: String,
    /// Bounded explanation without secrets or print contents.
    pub message: String,
    /// Operational severity.
    pub severity: DiagnosticSeverity,
}

impl Validate for DiagnosticIssue {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_identifier(&self.code, "code", 128)?;
        validate_string(&self.message, "message", 1, 1_024)
    }
}

/// Bounded operational summary safe to send to the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentDiagnostics {
    /// Stable installation identity.
    pub agent_id: String,
    /// UTC collection time.
    pub collected_at: String,
    /// Coarse health state.
    pub health: AgentHealth,
    /// Number of jobs waiting locally.
    pub queue_depth: u64,
    /// Number of printers currently online.
    pub printers_online: u16,
    /// Number of known printers.
    pub printers_total: u16,
    /// Sanitized bounded issue list.
    pub issues: Vec<DiagnosticIssue>,
}

impl Validate for AgentDiagnostics {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_identifier(&self.agent_id, "agentId", 128)?;
        validate_timestamp(&self.collected_at, "collectedAt")?;
        validate_safe_integer(self.queue_depth, "queueDepth")?;
        if usize::from(self.printers_online) > MAX_PRINTERS_PER_INVENTORY
            || usize::from(self.printers_total) > MAX_PRINTERS_PER_INVENTORY
        {
            return Err(ValidationError::new(
                "printersTotal",
                format!("printer counts must not exceed {MAX_PRINTERS_PER_INVENTORY}"),
            ));
        }
        if self.issues.len() > 64 {
            return Err(ValidationError::new(
                "issues",
                "must contain at most 64 issues",
            ));
        }
        for (index, issue) in self.issues.iter().enumerate() {
            issue
                .validate()
                .map_err(|error| error.at(format!("issues.{index}")))?;
        }
        Ok(())
    }
}

/// Discriminated payload for any agent-to-server message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum AgentMessageKind {
    /// Initial agent handshake.
    #[serde(rename = "agent.hello")]
    Hello(AgentHello),
    /// Heartbeat response.
    #[serde(rename = "agent.heartbeat")]
    Heartbeat(AgentHeartbeat),
    /// Complete printer inventory.
    #[serde(rename = "agent.printer_inventory")]
    PrinterInventory(PrinterInventory),
    /// Incremental printer inventory update.
    #[serde(rename = "agent.printer_inventory_changed")]
    PrinterInventoryChanged(PrinterInventoryChanged),
    /// Durable job acknowledgement.
    #[serde(rename = "agent.job_received")]
    JobReceived(JobReceived),
    /// Backend submission result.
    #[serde(rename = "agent.job_submitted")]
    JobSubmitted(JobSubmitted),
    /// Job failure result.
    #[serde(rename = "agent.job_failed")]
    JobFailed(JobFailed),
    /// Sanitized agent diagnostics.
    #[serde(rename = "agent.diagnostics")]
    Diagnostics(AgentDiagnostics),
}

impl AgentMessageKind {
    /// Returns the stable wire discriminator.
    pub const fn message_type(&self) -> &'static str {
        match self {
            Self::Hello(_) => "agent.hello",
            Self::Heartbeat(_) => "agent.heartbeat",
            Self::PrinterInventory(_) => "agent.printer_inventory",
            Self::PrinterInventoryChanged(_) => "agent.printer_inventory_changed",
            Self::JobReceived(_) => "agent.job_received",
            Self::JobSubmitted(_) => "agent.job_submitted",
            Self::JobFailed(_) => "agent.job_failed",
            Self::Diagnostics(_) => "agent.diagnostics",
        }
    }
}

impl Validate for AgentMessageKind {
    fn validate(&self) -> Result<(), ValidationError> {
        match self {
            Self::Hello(payload) => payload.validate(),
            Self::Heartbeat(payload) => payload.validate(),
            Self::PrinterInventory(payload) => payload.validate(),
            Self::PrinterInventoryChanged(payload) => payload.validate(),
            Self::JobReceived(payload) => payload.validate(),
            Self::JobSubmitted(payload) => payload.validate(),
            Self::JobFailed(payload) => payload.validate(),
            Self::Diagnostics(payload) => payload.validate(),
        }
        .map_err(|error| error.at("payload"))
    }
}

/// Versioned envelope for an agent-to-server message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMessage {
    /// Validated wire protocol version.
    pub protocol_version: ProtocolVersion,
    /// Unique message identity.
    pub message_id: String,
    /// UTC message creation time.
    pub sent_at: String,
    /// Request message identity for correlated responses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    /// Discriminated message payload.
    #[serde(flatten)]
    pub kind: AgentMessageKind,
}

impl AgentMessage {
    /// Returns the stable message discriminator.
    pub const fn message_type(&self) -> &'static str {
        self.kind.message_type()
    }
}

impl Validate for AgentMessage {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_envelope(
            &self.message_id,
            &self.sent_at,
            self.correlation_id.as_deref(),
        )?;
        let correlation = self.correlation_id.is_some();
        match self.kind {
            AgentMessageKind::Heartbeat(_)
            | AgentMessageKind::JobReceived(_)
            | AgentMessageKind::JobSubmitted(_)
            | AgentMessageKind::JobFailed(_)
                if !correlation =>
            {
                return Err(ValidationError::new(
                    "correlationId",
                    "is required for this response",
                ));
            }
            AgentMessageKind::Hello(_)
            | AgentMessageKind::PrinterInventoryChanged(_)
            | AgentMessageKind::Diagnostics(_)
                if correlation =>
            {
                return Err(ValidationError::new(
                    "correlationId",
                    "is not allowed for this event",
                ));
            }
            _ => {}
        }
        self.kind.validate()
    }
}

/// Human-readable identity advertised by an OpenPrinter service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenPrinterBrandMetadata {
    /// Service brand shown by the agent, for example `Acme POS`.
    pub name: String,
}

impl Validate for OpenPrinterBrandMetadata {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_brand_name(&self.name, "brand.name")
    }
}

/// Server handshake response and selected protocol version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServerHello {
    /// Stable server instance identity.
    pub server_id: String,
    /// Running server SDK version.
    pub server_version: String,
    /// Human-readable service identity safe to display in the agent UI.
    pub brand: OpenPrinterBrandMetadata,
    /// Connection session identity.
    pub session_id: String,
    /// Protocol versions understood by the server.
    pub supported_protocol_versions: Vec<ProtocolVersion>,
    /// Version selected for this connection.
    pub selected_protocol_version: ProtocolVersion,
    /// Expected heartbeat cadence.
    pub heartbeat_interval_ms: u32,
    /// Server-specific limit no greater than the protocol hard limit.
    pub max_message_bytes: u32,
}

impl Validate for ServerHello {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_identifier(&self.server_id, "serverId", 128)?;
        validate_string(&self.server_version, "serverVersion", 1, 256)?;
        self.brand.validate()?;
        validate_identifier(&self.session_id, "sessionId", 128)?;
        validate_versions(
            &self.supported_protocol_versions,
            "supportedProtocolVersions",
        )?;
        if !(5_000..=300_000).contains(&self.heartbeat_interval_ms) {
            return Err(ValidationError::new(
                "heartbeatIntervalMs",
                "must be between 5000 and 300000",
            ));
        }
        if !(1_024..=MAX_WIRE_MESSAGE_BYTES as u32).contains(&self.max_message_bytes) {
            return Err(ValidationError::new(
                "maxMessageBytes",
                format!("must be between 1024 and {MAX_WIRE_MESSAGE_BYTES}"),
            ));
        }
        Ok(())
    }
}

/// Server liveness probe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HeartbeatRequest {
    /// Response timeout selected by the server.
    pub timeout_ms: u32,
}

impl Validate for HeartbeatRequest {
    fn validate(&self) -> Result<(), ValidationError> {
        if !(1_000..=120_000).contains(&self.timeout_ms) {
            return Err(ValidationError::new(
                "timeoutMs",
                "must be between 1000 and 120000",
            ));
        }
        Ok(())
    }
}

/// Request cancellation before backend submission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CancelJob {
    /// Durable job identity to cancel.
    pub job_id: String,
    /// Optional bounded explanation for diagnostics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl Validate for CancelJob {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_identifier(&self.job_id, "jobId", 128)?;
        if let Some(reason) = &self.reason {
            validate_string(reason, "reason", 1, 512)?;
        }
        Ok(())
    }
}

/// Empty payload requesting a complete printer inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestPrinterInventory {}

impl Validate for RequestPrinterInventory {
    fn validate(&self) -> Result<(), ValidationError> {
        Ok(())
    }
}

/// Configuration area invalidated by the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfigurationScope {
    /// Agent-wide remote configuration.
    Agent,
    /// Printer-related remote configuration.
    Printers,
    /// All remotely supplied configuration.
    All,
}

/// Notification that host-owned configuration should be refreshed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigurationInvalidated {
    /// Configuration area to refresh.
    pub scope: ConfigurationScope,
    /// Optional opaque configuration revision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
}

impl Validate for ConfigurationInvalidated {
    fn validate(&self) -> Result<(), ValidationError> {
        if let Some(revision) = &self.revision {
            validate_string(revision, "revision", 1, 128)?;
        }
        Ok(())
    }
}

/// Intentional server disconnect and reconnection policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Disconnect {
    /// Stable machine-readable reason.
    pub code: String,
    /// Bounded human-readable explanation.
    pub reason: String,
    /// Whether the agent should reconnect automatically.
    pub reconnect: bool,
    /// Optional bounded reconnection delay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u32>,
}

impl Validate for Disconnect {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_identifier(&self.code, "code", 128)?;
        validate_string(&self.reason, "reason", 1, 1_024)?;
        if matches!(self.retry_after_ms, Some(value) if value > 86_400_000) {
            return Err(ValidationError::new(
                "retryAfterMs",
                "must be no greater than 86400000",
            ));
        }
        Ok(())
    }
}

/// Discriminated payload for any server-to-agent message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum ServerMessageKind {
    /// Server handshake response.
    #[serde(rename = "server.hello")]
    Hello(ServerHello),
    /// Heartbeat request.
    #[serde(rename = "server.heartbeat")]
    Heartbeat(HeartbeatRequest),
    /// At-least-once delivery of a concrete print job.
    #[serde(rename = "server.print_job")]
    PrintJob(PrintJob),
    /// Request cancellation before submission.
    #[serde(rename = "server.cancel_job")]
    CancelJob(CancelJob),
    /// Request a complete printer inventory.
    #[serde(rename = "server.request_printer_inventory")]
    RequestPrinterInventory(RequestPrinterInventory),
    /// Notify the agent of stale host-owned configuration.
    #[serde(rename = "server.configuration_invalidated")]
    ConfigurationInvalidated(ConfigurationInvalidated),
    /// Explain an intentional disconnect.
    #[serde(rename = "server.disconnect")]
    Disconnect(Disconnect),
}

impl ServerMessageKind {
    /// Returns the stable wire discriminator.
    pub const fn message_type(&self) -> &'static str {
        match self {
            Self::Hello(_) => "server.hello",
            Self::Heartbeat(_) => "server.heartbeat",
            Self::PrintJob(_) => "server.print_job",
            Self::CancelJob(_) => "server.cancel_job",
            Self::RequestPrinterInventory(_) => "server.request_printer_inventory",
            Self::ConfigurationInvalidated(_) => "server.configuration_invalidated",
            Self::Disconnect(_) => "server.disconnect",
        }
    }
}

impl Validate for ServerMessageKind {
    fn validate(&self) -> Result<(), ValidationError> {
        match self {
            Self::Hello(payload) => payload.validate(),
            Self::Heartbeat(payload) => payload.validate(),
            Self::PrintJob(payload) => payload.validate(),
            Self::CancelJob(payload) => payload.validate(),
            Self::RequestPrinterInventory(payload) => payload.validate(),
            Self::ConfigurationInvalidated(payload) => payload.validate(),
            Self::Disconnect(payload) => payload.validate(),
        }
        .map_err(|error| error.at("payload"))
    }
}

/// Versioned envelope for a server-to-agent message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerMessage {
    /// Validated wire protocol version.
    pub protocol_version: ProtocolVersion,
    /// Unique message identity.
    pub message_id: String,
    /// UTC message creation time.
    pub sent_at: String,
    /// Agent request identity for the correlated server hello.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    /// Discriminated message payload.
    #[serde(flatten)]
    pub kind: ServerMessageKind,
}

impl ServerMessage {
    /// Returns the stable message discriminator.
    pub const fn message_type(&self) -> &'static str {
        self.kind.message_type()
    }
}

impl Validate for ServerMessage {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_envelope(
            &self.message_id,
            &self.sent_at,
            self.correlation_id.as_deref(),
        )?;
        match (&self.kind, self.correlation_id.is_some()) {
            (ServerMessageKind::Hello(_), false) => {
                return Err(ValidationError::new(
                    "correlationId",
                    "is required for the server hello response",
                ));
            }
            (ServerMessageKind::Hello(_), true) => {}
            (_, true) => {
                return Err(ValidationError::new(
                    "correlationId",
                    "is not allowed for this server command",
                ));
            }
            (_, false) => {}
        }
        self.kind.validate()
    }
}

/// A validated message in either protocol direction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolMessage {
    /// Agent-to-server message.
    Agent(AgentMessage),
    /// Server-to-agent message.
    Server(ServerMessage),
}

impl ProtocolMessage {
    /// Returns the stable message discriminator.
    pub const fn message_type(&self) -> &'static str {
        match self {
            Self::Agent(message) => message.message_type(),
            Self::Server(message) => message.message_type(),
        }
    }
}

impl Validate for ProtocolMessage {
    fn validate(&self) -> Result<(), ValidationError> {
        match self {
            Self::Agent(message) => message.validate(),
            Self::Server(message) => message.validate(),
        }
    }
}

fn validate_envelope(
    message_id: &str,
    sent_at: &str,
    correlation_id: Option<&str>,
) -> Result<(), ValidationError> {
    validate_identifier(message_id, "messageId", 128)?;
    validate_timestamp(sent_at, "sentAt")?;
    if let Some(correlation_id) = correlation_id {
        validate_identifier(correlation_id, "correlationId", 128)?;
    }
    Ok(())
}

fn validate_versions(versions: &[ProtocolVersion], path: &str) -> Result<(), ValidationError> {
    if versions.is_empty() || versions.len() > 8 {
        return Err(ValidationError::new(
            path,
            "must contain between 1 and 8 versions",
        ));
    }
    if versions.len() > 1 {
        return Err(ValidationError::new(
            path,
            "must not contain duplicate versions",
        ));
    }
    Ok(())
}

fn validate_safe_integer(value: u64, path: &str) -> Result<(), ValidationError> {
    if value > MAX_SAFE_INTEGER {
        return Err(ValidationError::new(
            path,
            "must not exceed the JavaScript safe integer range",
        ));
    }
    Ok(())
}

fn validate_printers(printers: &[PrinterDescriptor], path: &str) -> Result<(), ValidationError> {
    if printers.len() > MAX_PRINTERS_PER_INVENTORY {
        return Err(ValidationError::new(
            path,
            format!("must contain at most {MAX_PRINTERS_PER_INVENTORY} printers"),
        ));
    }
    for (index, printer) in printers.iter().enumerate() {
        printer
            .validate()
            .map_err(|error| error.at(format!("{path}.{index}")))?;
    }
    Ok(())
}

fn validate_job_identity(job_id: &str, idempotency_key: &str) -> Result<(), ValidationError> {
    validate_identifier(job_id, "jobId", 128)?;
    validate_string(idempotency_key, "idempotencyKey", 1, 256)
}

fn validate_expected_status(actual: JobStatus, expected: JobStatus) -> Result<(), ValidationError> {
    if actual != expected {
        return Err(ValidationError::new(
            "status",
            "does not match the message discriminator",
        ));
    }
    Ok(())
}
