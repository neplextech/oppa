//! Printer-domain types shared by discovery, rendering, spoolers, and storage.
//!
//! A [`PrinterId`] is OPPA's stable local identity. A
//! [`PrinterFingerprint`] is only evidence used to recognize a device and may
//! change as drivers or network addresses change. In particular, a MAC address
//! is never treated as a universal identity.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::{collections::BTreeMap, time::Duration};

use async_trait::async_trait;
pub use oppa_core::PrinterId;
use oppa_core::Timestamp;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Upper bound for a user-visible printer name.
pub const MAX_PRINTER_NAME_BYTES: usize = 256;

/// How OPPA can reach a printer.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum PrinterConnection {
    /// A queue managed by the host operating system.
    SystemQueue {
        /// Exact queue identifier passed to the platform spooler.
        queue_name: String,
    },
    /// A raw network printer endpoint, commonly port 9100.
    Network {
        /// DNS name or IP address. This is never interpreted as a URL.
        host: String,
        /// TCP service port.
        port: u16,
    },
    /// A USB printer descriptor.
    Usb {
        /// USB vendor identifier.
        vendor_id: u16,
        /// USB product identifier.
        product_id: u16,
        /// Device serial number when exposed by the operating system.
        serial_number: Option<String>,
    },
    /// An in-process virtual printer.
    Virtual {
        /// Stable backend-local identifier.
        printer_id: String,
    },
}

/// Language carried by a raw printer submission.
///
/// This describes the bytes sent over a connection. It is intentionally
/// separate from [`PrinterConnection`], which only describes how the printer
/// is reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PrinterLanguage {
    /// ESC/POS receipt-printer commands.
    EscPos,
}

/// How rendered content is submitted to a configured printer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "language", rename_all = "kebab-case")]
pub enum SubmissionMode {
    /// Render a device-independent page and let the operating-system driver
    /// translate it for the printer.
    #[default]
    Driver,
    /// Send printer-language bytes directly through the selected transport.
    Raw(PrinterLanguage),
}

/// The concrete printer class emulated by an in-process virtual printer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum VirtualPrinterProfile {
    /// Thermal receipt printer that consumes raw ESC/POS.
    EscPosReceipt {
        /// Receipt width in millimetres. Supported values are 58 and 80.
        width_mm: u16,
    },
    /// Office printer that consumes a page for driver-based printing.
    SystemDriverPage {
        /// Page width in millimetres.
        page_width_mm: u16,
        /// Page height in millimetres.
        page_height_mm: u16,
        /// Raster resolution used by the virtual and real driver paths.
        dpi: u16,
    },
}

impl VirtualPrinterProfile {
    /// Validates bounded page dimensions and resolution.
    pub fn validate(&self) -> Result<(), PrinterValidationError> {
        match self {
            Self::EscPosReceipt { width_mm: 58 | 80 } => Ok(()),
            Self::EscPosReceipt { .. } => Err(PrinterValidationError::InvalidVirtualProfile),
            Self::SystemDriverPage {
                page_width_mm,
                page_height_mm,
                dpi,
            } if (100..=500).contains(page_width_mm)
                && (100..=500).contains(page_height_mm)
                && (72..=600).contains(dpi) =>
            {
                Ok(())
            }
            Self::SystemDriverPage { .. } => Err(PrinterValidationError::InvalidVirtualProfile),
        }
    }
}

impl PrinterConnection {
    /// Validates fields before a connection is saved or used.
    pub fn validate(&self) -> Result<(), PrinterValidationError> {
        match self {
            Self::SystemQueue { queue_name } => {
                validate_text("queue name", queue_name, MAX_PRINTER_NAME_BYTES)
            }
            Self::Network { host, port } => {
                validate_text("network host", host, 253)?;
                if host.contains("://") || host.contains('/') {
                    return Err(PrinterValidationError::InvalidHost);
                }
                if *port == 0 {
                    return Err(PrinterValidationError::InvalidPort);
                }
                Ok(())
            }
            Self::Usb { serial_number, .. } => {
                if let Some(serial_number) = serial_number {
                    validate_text("USB serial number", serial_number, 256)?;
                }
                Ok(())
            }
            Self::Virtual { printer_id } => {
                validate_text("virtual printer id", printer_id, MAX_PRINTER_NAME_BYTES)
            }
        }
    }

    /// Returns the stable connection family used to select a spooler.
    #[must_use]
    pub const fn kind(&self) -> ConnectionKind {
        match self {
            Self::SystemQueue { .. } => ConnectionKind::SystemQueue,
            Self::Network { .. } => ConnectionKind::Network,
            Self::Usb { .. } => ConnectionKind::Usb,
            Self::Virtual { .. } => ConnectionKind::Virtual,
        }
    }
}

/// Discriminator for a printer connection without its sensitive details.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConnectionKind {
    /// Operating-system queue.
    SystemQueue,
    /// Network endpoint.
    Network,
    /// Direct USB device.
    Usb,
    /// In-process development printer.
    Virtual,
}

/// Observed properties used to recognize a printer across discovery passes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrinterFingerprint {
    /// Operating-system queue name.
    pub queue_name: Option<String>,
    /// Installed driver name.
    pub driver_name: Option<String>,
    /// Platform device URI.
    pub device_uri: Option<String>,
    /// Network host or address.
    pub host: Option<String>,
    /// Network service port.
    pub port: Option<u16>,
    /// USB vendor identifier.
    pub usb_vendor_id: Option<u16>,
    /// USB product identifier.
    pub usb_product_id: Option<u16>,
    /// Device serial number.
    pub serial_number: Option<String>,
    /// MAC address when a trustworthy provider reports it.
    pub mac_address: Option<String>,
}

impl PrinterFingerprint {
    /// Returns normalized evidence keys, strongest first.
    ///
    /// These keys are useful for deduplication, but durable identity must still
    /// be assigned and retained by OPPA.
    #[must_use]
    pub fn identity_keys(&self) -> Vec<String> {
        let mut keys = Vec::new();
        if let (Some(vendor), Some(product), Some(serial)) = (
            self.usb_vendor_id,
            self.usb_product_id,
            normalized(&self.serial_number),
        ) {
            keys.push(format!("usb:{vendor:04x}:{product:04x}:{serial}"));
        }
        if let Some(uri) = normalized(&self.device_uri) {
            keys.push(format!("uri:{uri}"));
        }
        if let Some(host) = normalized(&self.host) {
            keys.push(format!("network:{host}:{}", self.port.unwrap_or(9100)));
        }
        if let Some(queue) = normalized(&self.queue_name) {
            keys.push(format!("queue:{queue}"));
        }
        if let Some(mac) = normalized(&self.mac_address) {
            keys.push(format!("mac:{mac}"));
        }
        keys
    }

    /// Fills missing observations from another provider without overwriting
    /// already selected values.
    pub fn merge_missing(&mut self, other: &Self) {
        merge_option(&mut self.queue_name, &other.queue_name);
        merge_option(&mut self.driver_name, &other.driver_name);
        merge_option(&mut self.device_uri, &other.device_uri);
        merge_option(&mut self.host, &other.host);
        merge_option(&mut self.port, &other.port);
        merge_option(&mut self.usb_vendor_id, &other.usb_vendor_id);
        merge_option(&mut self.usb_product_id, &other.usb_product_id);
        merge_option(&mut self.serial_number, &other.serial_number);
        merge_option(&mut self.mac_address, &other.mac_address);
    }
}

fn merge_option<T: Clone>(destination: &mut Option<T>, source: &Option<T>) {
    if destination.is_none() {
        destination.clone_from(source);
    }
}

fn normalized(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_lowercase)
}

/// Broad printer purpose discovered or configured locally.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PrinterKind {
    /// Receipt printer.
    Receipt,
    /// Label printer.
    Label,
    /// General document printer.
    Document,
    /// Development-only virtual target.
    Virtual,
    /// The provider could not infer a purpose.
    #[default]
    Unknown,
}

/// Current best-effort availability reported by local providers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PrinterAvailability {
    /// The provider reports the printer ready.
    Online,
    /// The provider reports the printer unavailable.
    Offline,
    /// The provider reports a transient error or attention condition.
    Degraded,
    /// Availability could not be established.
    #[default]
    Unknown,
}

/// Printer features that OPPA can safely advertise.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrinterCapabilities {
    /// Supported receipt widths in millimetres.
    pub receipt_widths_mm: Vec<u16>,
    /// Whether the selected backend accepts ESC/POS bytes.
    pub esc_pos: bool,
    /// Whether device-independent pages can be sent through a system driver.
    #[serde(default)]
    pub system_driver: bool,
    /// Whether raster documents can be submitted.
    pub raster: bool,
    /// Whether the device has an automatic cutter.
    pub cut: bool,
    /// Whether QR code output is supported through the renderer/backend pair.
    pub qr_code: bool,
    /// Whether barcode output is supported through the renderer/backend pair.
    pub barcode: bool,
    /// Whether cancellation is supported before backend acceptance.
    pub cancellation: bool,
}

/// One provider's observation retained during deduplication.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderMetadata {
    /// Stable provider name, such as `system-queue` or `manual-network`.
    pub provider: String,
    /// Provider-local identifier when available.
    pub provider_id: Option<String>,
    /// Sanitized provider-specific attributes.
    #[serde(default)]
    pub attributes: BTreeMap<String, String>,
}

/// A discovered printer before or after local identity assignment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoveredPrinter {
    /// Previously assigned stable identity, if this observation was reconciled.
    pub id: Option<PrinterId>,
    /// Human-readable local name.
    pub name: String,
    /// Best-effort printer purpose.
    pub kind: PrinterKind,
    /// Connection through which the printer can be used.
    pub connection: PrinterConnection,
    /// Observed identity evidence.
    pub fingerprint: PrinterFingerprint,
    /// Current availability.
    pub availability: PrinterAvailability,
    /// Capabilities reported during discovery, if known.
    pub capabilities: Option<PrinterCapabilities>,
    /// Metadata from every provider merged into this observation.
    #[serde(default)]
    pub providers: Vec<ProviderMetadata>,
}

impl DiscoveredPrinter {
    /// Validates fields received from a discovery provider.
    pub fn validate(&self) -> Result<(), PrinterValidationError> {
        validate_text("printer name", &self.name, MAX_PRINTER_NAME_BYTES)?;
        self.connection.validate()?;
        if self.providers.is_empty() {
            return Err(PrinterValidationError::MissingProvider);
        }
        for provider in &self.providers {
            validate_text("provider name", &provider.provider, 64)?;
        }
        Ok(())
    }
}

/// A configured printer reference safe to persist and pass to spoolers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrinterRef {
    /// OPPA-assigned stable local identity.
    pub id: PrinterId,
    /// Local display name.
    pub display_name: String,
    /// Selected connection.
    pub connection: PrinterConnection,
    /// How rendered content is submitted through the selected connection.
    #[serde(default)]
    pub submission_mode: SubmissionMode,
    /// Emulated device profile, present only for virtual printers.
    #[serde(default)]
    pub virtual_profile: Option<VirtualPrinterProfile>,
    /// Whether the agent may advertise and submit to this printer.
    pub enabled: bool,
}

impl PrinterRef {
    /// Validates a persisted or user-provided reference before use.
    pub fn validate(&self) -> Result<(), PrinterValidationError> {
        validate_text(
            "printer display name",
            &self.display_name,
            MAX_PRINTER_NAME_BYTES,
        )?;
        self.connection
            .validate()
            .and_then(|()| match (&self.connection, self.virtual_profile) {
                (PrinterConnection::Virtual { .. }, Some(profile)) => {
                    profile.validate()?;
                    match (profile, self.submission_mode) {
                        (
                            VirtualPrinterProfile::EscPosReceipt { .. },
                            SubmissionMode::Raw(PrinterLanguage::EscPos),
                        )
                        | (
                            VirtualPrinterProfile::SystemDriverPage { .. },
                            SubmissionMode::Driver,
                        ) => Ok(()),
                        _ => Err(PrinterValidationError::IncompatibleVirtualMode),
                    }
                }
                (PrinterConnection::Virtual { .. }, None) => {
                    Err(PrinterValidationError::MissingVirtualProfile)
                }
                (_, Some(_)) => Err(PrinterValidationError::UnexpectedVirtualProfile),
                _ => Ok(()),
            })
    }
}

/// Backend acknowledgement that a job was accepted for submission.
///
/// This receipt never represents proof of physical output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubmissionReceipt {
    /// Backend-assigned queue or submission identifier, if one exists.
    pub backend_job_id: Option<String>,
    /// Backend family that accepted the document.
    pub backend: String,
    /// Acceptance time.
    pub accepted_at: Timestamp,
    /// Sanitized backend metadata.
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

/// Validation failures for printer inputs.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PrinterValidationError {
    /// A required field was blank.
    #[error("{field} must not be empty")]
    Empty {
        /// Name of the invalid field.
        field: &'static str,
    },
    /// A text value exceeded its documented limit.
    #[error("{field} exceeds the {maximum}-byte limit")]
    TooLong {
        /// Name of the invalid field.
        field: &'static str,
        /// Maximum UTF-8 byte length.
        maximum: usize,
    },
    /// A host looked like a URL or contained a path.
    #[error("network host must be a hostname or IP address, not a URL or path")]
    InvalidHost,
    /// TCP port zero was supplied.
    #[error("network printer port must be between 1 and 65535")]
    InvalidPort,
    /// Discovery data did not identify its provider.
    #[error("discovered printer must preserve at least one provider observation")]
    MissingProvider,
    /// A virtual printer did not identify the device it emulates.
    #[error("virtual printer must define an emulation profile")]
    MissingVirtualProfile,
    /// A physical printer was configured with a virtual device profile.
    #[error("physical printers cannot define a virtual emulation profile")]
    UnexpectedVirtualProfile,
    /// The virtual profile and submission mode describe different devices.
    #[error("virtual printer profile and submission mode are incompatible")]
    IncompatibleVirtualMode,
    /// A virtual profile contains unsupported dimensions or resolution.
    #[error("virtual printer profile has unsupported dimensions or resolution")]
    InvalidVirtualProfile,
}

fn validate_text(
    field: &'static str,
    value: &str,
    maximum: usize,
) -> Result<(), PrinterValidationError> {
    if value.trim().is_empty() || value.trim() != value {
        return Err(PrinterValidationError::Empty { field });
    }
    if value.len() > maximum {
        return Err(PrinterValidationError::TooLong { field, maximum });
    }
    Ok(())
}

/// Failures returned by printer discovery/capability backends.
#[derive(Debug, Error)]
pub enum PrinterBackendError {
    /// This backend is unavailable on the current platform or build.
    #[error("printer backend is unavailable: {0}")]
    Unavailable(String),
    /// Provider data failed validation.
    #[error(transparent)]
    InvalidPrinter(#[from] PrinterValidationError),
    /// A backend operation timed out.
    #[error("printer backend timed out after {0:?}")]
    Timeout(Duration),
    /// A sanitized backend failure.
    #[error("printer backend failed: {0}")]
    Backend(String),
}

/// Result alias for printer inspection backends.
pub type PrinterBackendResult<T> = Result<T, PrinterBackendError>;

/// Low-level discovery and capability contract.
///
/// Submission lives in `oppa-spooler` so this leaf crate does not depend on
/// renderer output types.
#[async_trait]
pub trait PrinterBackend: Send + Sync {
    /// Discovers printer observations available through this backend.
    async fn discover(&self) -> PrinterBackendResult<Vec<DiscoveredPrinter>>;

    /// Retrieves capabilities for a configured printer.
    async fn capabilities(&self, printer: &PrinterRef)
    -> PrinterBackendResult<PrinterCapabilities>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_connections_reject_urls_and_port_zero() {
        assert_eq!(
            PrinterConnection::Network {
                host: "https://printer.local".to_owned(),
                port: 9100,
            }
            .validate(),
            Err(PrinterValidationError::InvalidHost)
        );
        assert_eq!(
            PrinterConnection::Network {
                host: "printer.local".to_owned(),
                port: 0,
            }
            .validate(),
            Err(PrinterValidationError::InvalidPort)
        );
    }

    #[test]
    fn fingerprint_keys_do_not_require_mac_addresses() {
        let fingerprint = PrinterFingerprint {
            queue_name: Some(" Receipt ".to_owned()),
            host: Some("PRINTER.LOCAL".to_owned()),
            port: Some(9100),
            ..PrinterFingerprint::default()
        };
        assert_eq!(
            fingerprint.identity_keys(),
            vec![
                "network:printer.local:9100".to_owned(),
                "queue:receipt".to_owned()
            ]
        );
    }

    #[test]
    fn connection_serialization_has_explicit_discriminator() {
        let connection = PrinterConnection::SystemQueue {
            queue_name: "receipts".to_owned(),
        };
        let value = serde_json::to_value(connection).expect("serialize connection");
        assert_eq!(value["type"], "system-queue");
        assert_eq!(value["queue_name"], "receipts");
    }

    #[test]
    fn submission_mode_defaults_to_driver_and_keeps_language_separate() {
        let legacy = serde_json::json!({
            "id": "printer_legacy",
            "displayName": "Office",
            "connection": {"type": "system-queue", "queue_name": "Office"},
            "enabled": true
        });
        let printer: PrinterRef = serde_json::from_value(legacy).expect("legacy printer");
        assert_eq!(printer.submission_mode, SubmissionMode::Driver);

        let raw = serde_json::to_value(SubmissionMode::Raw(PrinterLanguage::EscPos))
            .expect("serialize mode");
        assert_eq!(raw["type"], "raw");
        assert_eq!(raw["language"], "esc-pos");
    }

    #[test]
    fn virtual_profiles_are_validated_and_separate_from_connection_identity() {
        let printer = PrinterRef {
            id: PrinterId::new("virtual-office").expect("id"),
            display_name: "Virtual Office".to_owned(),
            connection: PrinterConnection::Virtual {
                printer_id: "virtual-office".to_owned(),
            },
            submission_mode: SubmissionMode::Driver,
            virtual_profile: Some(VirtualPrinterProfile::SystemDriverPage {
                page_width_mm: 210,
                page_height_mm: 297,
                dpi: 300,
            }),
            enabled: true,
        };
        assert!(printer.validate().is_ok());
    }
}
