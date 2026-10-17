//! Validated Rust representation of the OpenPrinter wire protocol.
//!
//! The TypeBox schemas in `packages/protocol` are canonical. This crate mirrors
//! their stable serialized names, enforces the same limits, and validates the
//! shared fixtures in `protocol/fixtures`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod auth;
mod codec;
mod document;
mod error;
mod job;
mod message;
mod negotiation;
mod pdf;
mod printer;
mod validation;

pub use auth::{
    AUTHENTICATION_METHOD, DISCOVERY_PATH, DiscoveryAuthentication, DiscoveryDocument,
    DiscoveryEndpoints, DiscoveryServer, GatewayAuthenticationAccepted,
    GatewayAuthenticationChallenge, GatewayAuthenticationRejected, GatewayAuthenticationResponse,
    GatewayAuthenticationServerMessage, PairingAgent, PairingCredential, PairingRequest,
    PairingResponse, PublicEd25519Jwk, SIGNATURE_ALGORITHM,
};
pub use codec::{
    decode_agent_message, decode_agent_message_for, decode_protocol_message, decode_server_message,
    decode_server_message_for, encode_agent_message, encode_protocol_message,
    encode_server_message,
};
pub use document::{
    BarcodeFormat, ImageMediaType, JobDocument, PrintDocument, PrintSection, ReceiptWidth,
    TextAlignment,
};
pub use error::{ProtocolError, ValidationError};
pub use job::PrintJob;
pub use message::{
    AgentDiagnostics, AgentHealth, AgentHeartbeat, AgentHello, AgentMessage, AgentMessageKind,
    CancelJob, ConfigurationInvalidated, ConfigurationScope, DiagnosticIssue, DiagnosticSeverity,
    Disconnect, FailureDetail, HeartbeatRequest, JobFailed, JobReceived, JobStatus, JobSubmitted,
    OpenPrinterBrandMetadata, PrinterInventory, PrinterInventoryChanged, ProtocolMessage,
    ProtocolVersion, RequestPrinterInventory, ServerHello, ServerMessage, ServerMessageKind,
};
pub use negotiation::{
    CacheUpdate, HandshakePlanner, LEGACY_RETRY_INTERVAL_MS, LegacyServerRecord, project_inventory,
    select_protocol_version,
};
pub use pdf::{
    CustomPaperSize, Duplex, MAX_COPIES, MAX_CUSTOM_PAPER_MM, MAX_PAGE_RANGE_BYTES,
    MAX_SCALE_PERCENT, MIN_COPIES, MIN_CUSTOM_PAPER_MM, MIN_SCALE_PERCENT, Orientation, PaperSize,
    PdfDocument, PdfFormat, PdfSource, PrintSettings, Scaling, StandardPaperSize,
    validate_document_url, validate_page_ranges,
};
pub use printer::{
    DocumentFormat, PrintOptionsCapability, PrinterAvailability, PrinterCapabilities,
    PrinterConnection, PrinterDescriptor, PrinterKind,
};
pub use validation::{Metadata, Validate};

/// The base wire protocol version. Every peer understands it, and it is the only version used by
/// authentication frames and discovery documents.
pub const PROTOCOL_VERSION: &str = "1";

/// Maximum UTF-8 size accepted by any protocol decoder.
pub const MAX_WIRE_MESSAGE_BYTES: usize = 2 * 1024 * 1024;

/// Maximum UTF-8 size accepted for a pre-handshake authentication frame.
pub const MAX_AUTH_MESSAGE_BYTES: usize = 16 * 1024;

/// Maximum number of primitives in one structured print document.
pub const MAX_DOCUMENT_SECTIONS: usize = 256;

/// Maximum number of printers accepted in an inventory snapshot.
pub const MAX_PRINTERS_PER_INVENTORY: usize = 512;

/// Maximum encoded image length accepted in a document.
pub const MAX_IMAGE_BASE64_LENGTH: usize = 1_398_104;

/// Maximum number of opaque string metadata entries carried by a job.
pub const MAX_METADATA_ENTRIES: usize = 32;

/// Maximum PDF size accepted for one job, in bytes.
pub const MAX_PDF_DOCUMENT_BYTES: u64 = 20 * 1024 * 1024;

/// Maximum UTF-8 length of a PDF retrieval URL.
pub const MAX_PDF_URL_BYTES: usize = 2_048;
