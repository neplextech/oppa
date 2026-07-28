use serde::{Deserialize, Serialize};

use crate::{
    Metadata, PrintDocument, Validate, ValidationError,
    validation::{validate_identifier, validate_metadata, validate_string, validate_timestamp},
};

/// A concrete, idempotent delivery request for one OPPA printer.
///
/// A manual reprint uses a new `job_id`; host-specific relationships may be
/// represented in bounded opaque metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrintJob {
    /// Durable host-generated identity for this delivery.
    pub job_id: String,
    /// Stable duplicate-detection key for at-least-once delivery.
    pub idempotency_key: String,
    /// Concrete OPPA printer selected by the integrating application.
    pub printer_id: String,
    /// UTC creation time supplied by the durable host queue.
    pub created_at: String,
    /// Printer-independent structured content.
    pub document: PrintDocument,
    /// Optional bounded application-owned routing context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

impl Validate for PrintJob {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_identifier(&self.job_id, "jobId", 128)?;
        validate_string(&self.idempotency_key, "idempotencyKey", 1, 256)?;
        validate_identifier(&self.printer_id, "printerId", 128)?;
        validate_timestamp(&self.created_at, "createdAt")?;
        self.document
            .validate()
            .map_err(|error| error.at("document"))?;
        if let Some(metadata) = &self.metadata {
            validate_metadata(metadata, "metadata")?;
        }
        Ok(())
    }
}
