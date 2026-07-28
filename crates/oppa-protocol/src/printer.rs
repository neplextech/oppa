use serde::{Deserialize, Serialize};

use crate::{
    ReceiptWidth, Validate, ValidationError,
    validation::{validate_identifier, validate_string},
};

/// How OPPA addresses a printer locally.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum PrinterConnection {
    /// Operating-system print queue.
    System {
        /// Stable queue name understood by the local backend.
        #[serde(rename = "systemName")]
        system_name: String,
    },
    /// Raw network printer endpoint.
    Tcp {
        /// DNS name or IP address discovered or configured locally.
        host: String,
        /// TCP service port.
        port: u16,
    },
    /// In-process development printer.
    Virtual,
}

impl Validate for PrinterConnection {
    fn validate(&self) -> Result<(), ValidationError> {
        match self {
            Self::System { system_name } => validate_string(system_name, "systemName", 1, 256),
            Self::Tcp { host, port } => {
                validate_string(host, "host", 1, 253)?;
                if *port == 0 {
                    return Err(ValidationError::new("port", "must be between 1 and 65535"));
                }
                Ok(())
            }
            Self::Virtual => Ok(()),
        }
    }
}

/// High-level printer origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrinterKind {
    /// Operating-system managed local printer.
    Local,
    /// Raw network printer.
    Network,
    /// In-process development printer.
    Virtual,
}

/// Current reachability state reported by OPPA.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrinterAvailability {
    /// Backend currently considers the printer reachable.
    Online,
    /// Backend currently considers the printer unreachable.
    Offline,
    /// Backend cannot determine reachability.
    Unknown,
}

/// Rendering and submission features reported by a printer backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrinterCapabilities {
    /// Receipt widths accepted by this backend.
    pub media_widths: Vec<ReceiptWidth>,
    /// Whether raster documents are supported.
    pub raster: bool,
    /// Whether a cut operation is supported.
    pub cut: bool,
    /// Whether QR code rendering is supported.
    pub qr: bool,
    /// Whether linear barcode rendering is supported.
    pub barcode: bool,
}

impl Validate for PrinterCapabilities {
    fn validate(&self) -> Result<(), ValidationError> {
        if self.media_widths.is_empty() || self.media_widths.len() > 2 {
            return Err(ValidationError::new(
                "mediaWidths",
                "must contain between 1 and 2 widths",
            ));
        }
        if self.media_widths.len() == 2 && self.media_widths[0] == self.media_widths[1] {
            return Err(ValidationError::new(
                "mediaWidths",
                "must not contain duplicate widths",
            ));
        }
        Ok(())
    }
}

/// A physical or virtual printer exposed by one OPPA agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrinterDescriptor {
    /// Stable agent-local printer identifier used by print jobs.
    pub id: String,
    /// Discovery fingerprint used to recognize the device across scans.
    pub fingerprint: String,
    /// Human-readable local display name.
    pub name: String,
    /// High-level printer origin.
    pub kind: PrinterKind,
    /// Backend-owned local connection description.
    pub connection: PrinterConnection,
    /// Supported rendering and submission features, when the backend can
    /// determine them without guessing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<PrinterCapabilities>,
    /// Whether the user permits job submission.
    pub enabled: bool,
    /// Current backend reachability.
    pub availability: PrinterAvailability,
}

impl Validate for PrinterDescriptor {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_identifier(&self.id, "id", 128)?;
        validate_string(&self.fingerprint, "fingerprint", 1, 256)?;
        validate_string(&self.name, "name", 1, 256)?;
        self.connection
            .validate()
            .map_err(|error| error.at("connection"))?;
        if let Some(capabilities) = &self.capabilities {
            capabilities
                .validate()
                .map_err(|error| error.at("capabilities"))?;
        }

        let kind_matches_connection = matches!(
            (&self.kind, &self.connection),
            (PrinterKind::Local, PrinterConnection::System { .. })
                | (PrinterKind::Network, PrinterConnection::Tcp { .. })
                | (PrinterKind::Virtual, PrinterConnection::Virtual)
        );
        if !kind_matches_connection {
            return Err(ValidationError::new(
                "connection",
                "must match the printer kind",
            ));
        }
        Ok(())
    }
}
