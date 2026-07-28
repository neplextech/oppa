use oppa_agent::AgentState;
use oppa_printer::{
    PrinterAvailability, PrinterCapabilities as DomainPrinterCapabilities, PrinterConnection,
    PrinterFingerprint, PrinterRef, ProviderMetadata,
};
use oppa_product::ProductConfig;
use serde::{Deserialize, Serialize};

/// Frontend projection of the compile-time product configuration.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub documentation_url: String,
    pub support_url: String,
    pub privacy_url: Option<String>,
    pub terms_url: Option<String>,
    pub legal_text: Option<String>,
}

impl From<&ProductConfig> for ProductSummary {
    fn from(product: &ProductConfig) -> Self {
        Self {
            id: product.product_id.to_string(),
            name: product.product_name.clone(),
            description: product.description.clone(),
            documentation_url: product.branding.documentation_url.to_string(),
            support_url: product.branding.support_url.to_string(),
            privacy_url: product.legal.privacy_url.as_ref().map(ToString::to_string),
            terms_url: product.legal.terms_url.as_ref().map(ToString::to_string),
            legal_text: product.legal.text.clone(),
        }
    }
}

/// Complete status returned to the desktop frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatus {
    pub agent_id: Option<String>,
    pub configured: bool,
    pub product: ProductSummary,
    pub state: AgentState,
    pub gateway_state: GatewayState,
    pub last_connection_at: Option<String>,
    pub version: String,
    pub pending_jobs: usize,
    pub active_errors: Vec<String>,
    pub start_on_login: bool,
    pub dashboard_url: Option<String>,
    pub platform: String,
}

/// Coarse gateway status expected by the frontend.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GatewayState {
    Offline,
    Connecting,
    Online,
}

/// Printer capability projection expected by the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrinterCapabilities {
    pub widths: Vec<u16>,
    pub document_types: Vec<DocumentType>,
    pub supports_cut: bool,
    pub supports_qr: bool,
}

/// Frontend document families.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentType {
    EscPos,
    Raster,
    Virtual,
}

/// Printer projection. Virtual-only fields are omitted for physical printers.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrinterSummary {
    pub id: String,
    pub display_name: String,
    pub source_name: String,
    pub connection_type: PrinterConnectionType,
    pub address: Option<String>,
    pub enabled: bool,
    pub available: bool,
    pub is_virtual: bool,
    pub capabilities: Option<PrinterCapabilities>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<VirtualPrinterMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<Vec<VirtualOutput>>,
}

/// Connection discriminator used by the frontend.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PrinterConnectionType {
    SystemQueue,
    Network,
    Virtual,
    Usb,
}

/// A virtual printer's simulation policy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VirtualPrinterMode {
    #[default]
    AlwaysSucceed,
    FailNext,
    AlwaysFail,
    Delay,
    Offline,
}

/// One bounded virtual output projection.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualOutput {
    pub id: String,
    pub job_id: String,
    pub created_at: String,
    pub format: VirtualOutputFormat,
    pub preview: String,
    pub byte_length: usize,
}

/// Rendered virtual output family.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VirtualOutputFormat {
    Structured,
    EscPos,
    Raster,
}

/// Persisted host-owned printer metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CatalogPrinter {
    pub reference: PrinterRef,
    pub source_name: String,
    pub availability: PrinterAvailability,
    pub capabilities: Option<DomainPrinterCapabilities>,
    #[serde(default)]
    pub fingerprint: PrinterFingerprint,
    #[serde(default)]
    pub providers: Vec<ProviderMetadata>,
    #[serde(default)]
    pub virtual_width: Option<u16>,
    #[serde(default)]
    pub virtual_mode: VirtualPrinterMode,
    #[serde(default)]
    pub virtual_delay_ms: u64,
}

impl CatalogPrinter {
    pub fn is_virtual(&self) -> bool {
        matches!(self.reference.connection, PrinterConnection::Virtual { .. })
    }
}

/// Durable desktop printer catalog stored as one bounded `SQLite` setting.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PersistedCatalog {
    pub printers: Vec<CatalogPrinter>,
    #[serde(default)]
    pub suppressed_system_ids: Vec<String>,
}

/// Partial printer changes accepted by `configure_printer`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigurePrinterChanges {
    pub display_name: Option<String>,
    pub enabled: Option<bool>,
}

/// Input for a raw TCP printer.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManualPrinterInput {
    pub display_name: String,
    pub host: String,
    pub port: u16,
}

/// Input for a virtual printer.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VirtualPrinterInput {
    pub display_name: String,
    pub width: u16,
}

/// Recent durable job projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobSummary {
    pub id: String,
    pub printer_id: String,
    pub printer_name: String,
    pub idempotency_key: String,
    pub state: DesktopJobState,
    pub received_at: String,
    pub updated_at: String,
    pub attempts: u32,
    pub error: Option<String>,
}

/// Frontend job lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DesktopJobState {
    Queued,
    Received,
    Submitted,
    Failed,
    Cancelled,
}

/// Browser authorization result returned immediately after opening the browser.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizationStart {
    pub authorization_url: String,
    pub expires_at: String,
}

/// Whitelisted compile-time product link.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductLink {
    Documentation,
    Support,
    Privacy,
    Terms,
}

/// Sanitized diagnostics projection.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    pub agent_version: String,
    pub product_id: String,
    pub platform: String,
    pub connection_state: AgentState,
    pub database_healthy: bool,
    pub migration_version: u32,
    pub discovery_providers: Vec<DiscoveryProviderStatus>,
    pub logs: Vec<DiagnosticLogEntry>,
}

/// One discovery provider's latest bounded status.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryProviderStatus {
    pub name: String,
    pub available: bool,
    pub last_scan_at: Option<String>,
    pub detail: String,
}

/// One bounded, sanitized desktop log entry.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticLogEntry {
    pub timestamp: String,
    pub level: DiagnosticLevel,
    pub target: String,
    pub message: String,
}

/// Diagnostic severity expected by the frontend.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticLevel {
    Info,
    Warn,
    Error,
}

/// Full local-only diagnostics export.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticExport {
    pub generated_at: String,
    pub diagnostics: Diagnostics,
    pub printers: Vec<PrinterSummary>,
    pub recent_jobs: Vec<JobSummary>,
    pub feature_availability: FeatureAvailability,
}

/// Compile-time and host feature availability without credentials or payloads.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_excessive_bools)]
pub struct FeatureAvailability {
    pub virtual_printer: bool,
    pub system_printer_discovery: bool,
    pub network_printer_discovery: bool,
    pub usb_printer_discovery: bool,
    pub remote_diagnostics: bool,
}
