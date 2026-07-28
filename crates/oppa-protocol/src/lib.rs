//! Validated Rust representation of the OpenPrinter wire protocol.
//!
//! The TypeBox schemas in `packages/protocol` are canonical. This crate mirrors
//! their stable serialized names, enforces the same limits, and validates the
//! shared fixtures in `protocol/fixtures`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod codec;
mod document;
mod error;
mod job;
mod message;
mod printer;
mod validation;

pub use codec::{
    decode_agent_message, decode_protocol_message, decode_server_message, encode_agent_message,
    encode_protocol_message, encode_server_message,
};
pub use document::{
    BarcodeFormat, ImageMediaType, PrintDocument, PrintSection, ReceiptWidth, TextAlignment,
};
pub use error::{ProtocolError, ValidationError};
pub use job::PrintJob;
pub use message::{
    AgentDiagnostics, AgentHealth, AgentHeartbeat, AgentHello, AgentMessage, AgentMessageKind,
    AuthenticationMetadata, AuthenticationMethod, CancelJob, ConfigurationInvalidated,
    ConfigurationScope, DiagnosticIssue, DiagnosticSeverity, Disconnect, FailureDetail,
    HeartbeatRequest, JobFailed, JobReceived, JobStatus, JobSubmitted, PrinterInventory,
    PrinterInventoryChanged, ProtocolMessage, ProtocolVersion, RequestPrinterInventory,
    ServerHello, ServerMessage, ServerMessageKind,
};
pub use printer::{
    PrinterAvailability, PrinterCapabilities, PrinterConnection, PrinterDescriptor, PrinterKind,
};
pub use validation::{Metadata, Validate};

/// The only wire protocol version understood by this release.
pub const PROTOCOL_VERSION: u16 = 1;

/// Maximum UTF-8 size accepted by any protocol decoder.
pub const MAX_WIRE_MESSAGE_BYTES: usize = 2 * 1024 * 1024;

/// Maximum number of primitives in one structured print document.
pub const MAX_DOCUMENT_SECTIONS: usize = 256;

/// Maximum number of printers accepted in an inventory snapshot.
pub const MAX_PRINTERS_PER_INVENTORY: usize = 512;

/// Maximum encoded image length accepted in a document.
pub const MAX_IMAGE_BASE64_LENGTH: usize = 1_398_104;

/// Maximum number of opaque string metadata entries carried by a job.
pub const MAX_METADATA_ENTRIES: usize = 32;
