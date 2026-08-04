use oppa_core::ProductId;
use serde::{Deserialize, Serialize};
use url::Url;

/// Product schema version implemented by this build.
pub const PRODUCT_SCHEMA_VERSION: u32 = 1;

/// Branding and provider configuration compiled into an OPPA binary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductConfig {
    /// Version of the product configuration schema.
    pub schema_version: u32,
    /// Stable product identifier.
    pub product_id: ProductId,
    /// Human-readable product name.
    pub product_name: String,
    /// Reverse-DNS application identifier.
    pub application_id: String,
    /// Short user-facing product description.
    pub description: String,
    /// Product update settings.
    pub updates: ProductUpdates,
    /// Support and documentation branding.
    pub branding: ProductBranding,
    /// Features enabled for this branded build.
    #[serde(default)]
    pub features: ProductFeatures,
    /// Optional product-specific legal links and text.
    #[serde(default)]
    pub legal: ProductLegal,
    /// Custom URL scheme used for deep link pairing (e.g. `"oppa-dev"`).
    ///
    /// Overrides the default `oppa` scheme. Must be a valid URL scheme: starts
    /// with an ASCII letter, followed by letters, digits, `+`, `-`, or `.`.
    /// Must not shadow a standard scheme (`http`, `https`, `file`, `ftp`).
    /// When absent, the scheme defaults to `"oppa"` at compile time.
    #[serde(default)]
    pub deep_link_scheme: Option<String>,
}

/// Product update configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductUpdates {
    /// Update manifest endpoint, or `None` when self-updates are disabled.
    pub endpoint: Option<Url>,
}

/// Product-owned links shown by the desktop host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductBranding {
    /// User support URL.
    pub support_url: Url,
    /// Product documentation URL.
    pub documentation_url: Url,
}

/// Optional product-owned legal content and links.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct ProductLegal {
    /// Product privacy policy URL.
    pub privacy_url: Option<Url>,
    /// Product terms of service URL.
    pub terms_url: Option<Url>,
    /// Short legal notice embedded into the application.
    pub text: Option<String>,
}

/// Product switches whose corresponding code must also be compiled in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct ProductFeatures {
    /// Expose the development and diagnostics virtual printer.
    pub virtual_printer: bool,
    /// Discover operating-system print queues.
    pub system_printer_discovery: bool,
    /// Permit manually configured or discovered network printers.
    pub network_printer_discovery: bool,
    /// Enumerate supported USB devices.
    pub usb_printer_discovery: bool,
    /// Permit sanitized diagnostics to be sent to the provider.
    pub remote_diagnostics: bool,
    /// Enable product-dedicated hardware behavior.
    pub managed_hardware: bool,
}

impl ProductConfig {
    /// Validates semantic constraints beyond JSON shape.
    ///
    /// Every issue is returned at once so a branded distributor can fix a
    /// configuration without repeated build attempts.
    #[must_use]
    pub fn validation_issues(&self) -> Vec<String> {
        let mut issues = Vec::new();

        if self.schema_version != PRODUCT_SCHEMA_VERSION {
            issues.push(format!(
                "schemaVersion {} is unsupported; this build supports {}",
                self.schema_version, PRODUCT_SCHEMA_VERSION
            ));
        }
        validate_required("productName", &self.product_name, 100, &mut issues);
        validate_required("description", &self.description, 500, &mut issues);
        validate_application_id(&self.application_id, &mut issues);
        validate_http_url(
            "branding.supportUrl",
            &self.branding.support_url,
            &mut issues,
        );
        validate_http_url(
            "branding.documentationUrl",
            &self.branding.documentation_url,
            &mut issues,
        );
        if let Some(endpoint) = &self.updates.endpoint {
            validate_http_url("updates.endpoint", endpoint, &mut issues);
        }
        if let Some(url) = &self.legal.privacy_url {
            validate_http_url("legal.privacyUrl", url, &mut issues);
        }
        if let Some(url) = &self.legal.terms_url {
            validate_http_url("legal.termsUrl", url, &mut issues);
        }
        if let Some(legal_text) = &self.legal.text {
            validate_required("legal.text", legal_text, 20_000, &mut issues);
        }
        if let Some(scheme) = &self.deep_link_scheme {
            validate_deep_link_scheme("deepLinkScheme", scheme, &mut issues);
        }

        issues
    }
}

fn validate_required(field: &str, value: &str, maximum: usize, issues: &mut Vec<String>) {
    if value.trim().is_empty() {
        issues.push(format!("{field} must not be empty"));
    } else if value.trim() != value {
        issues.push(format!("{field} must not have surrounding whitespace"));
    } else if value.len() > maximum {
        issues.push(format!("{field} must not exceed {maximum} bytes"));
    }
}

fn validate_application_id(value: &str, issues: &mut Vec<String>) {
    validate_required("applicationId", value, 255, issues);
    let segments = value.split('.').collect::<Vec<_>>();
    if segments.len() < 2
        || segments.iter().any(|segment| {
            segment.is_empty()
                || !segment
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '-')
                || !segment
                    .chars()
                    .next()
                    .is_some_and(|character| character.is_ascii_alphabetic())
        })
    {
        issues.push(
            "applicationId must be a reverse-DNS identifier with alphabetic segment starts"
                .to_owned(),
        );
    }
}

fn is_loopback(url: &Url) -> bool {
    matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1"))
}

fn validate_deep_link_scheme(field: &str, value: &str, issues: &mut Vec<String>) {
    if value.is_empty() {
        issues.push(format!("{field} must not be empty"));
        return;
    }
    let mut chars = value.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_alphabetic());
    let rest_ok = chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if !first_ok || !rest_ok {
        issues.push(format!(
            "{field} must be a valid URL scheme: letter start, then letters/digits/+/-/."
        ));
    }
    if matches!(value, "http" | "https" | "file" | "ftp") {
        issues.push(format!("{field} must not shadow a standard URL scheme"));
    }
}

fn validate_http_url(field: &str, value: &Url, issues: &mut Vec<String>) {
    let valid = value.scheme() == "https" || (value.scheme() == "http" && is_loopback(value));
    if !valid {
        issues.push(format!(
            "{field} must use HTTPS (HTTP is allowed only for a loopback development endpoint)"
        ));
    }
    if value.username() != "" || value.password().is_some() {
        issues.push(format!("{field} must not contain embedded credentials"));
    }
    if value.fragment().is_some() {
        issues.push(format!("{field} must not contain a URL fragment"));
    }
}
