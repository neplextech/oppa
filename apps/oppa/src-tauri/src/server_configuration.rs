use std::net::IpAddr;

use oppa_auth::AuthorizationEndpoints;
use oppa_product::ProductConfig;
use serde::{Deserialize, Serialize};
use url::{Host, Url};

use crate::error::CommandError;

pub const SERVER_CONFIGURATION_SETTING: &str = "openprinter-server-configuration-v1";
const MAX_ENDPOINT_BYTES: usize = 2_048;

/// Validated, non-secret `OpenPrinter` service endpoints persisted by the desktop host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[allow(clippy::struct_field_names)]
pub struct OpenPrinterServerConfiguration {
    pub authorization_url: Url,
    pub token_url: Url,
    pub gateway_url: Url,
}

impl OpenPrinterServerConfiguration {
    pub fn from_product(product: &ProductConfig) -> Self {
        Self {
            authorization_url: product.protocol.authorization_url.clone(),
            token_url: product.protocol.token_url.clone(),
            gateway_url: product.protocol.gateway_url.clone(),
        }
    }

    pub fn from_input(input: &OpenPrinterServerConfigurationInput) -> Result<Self, CommandError> {
        let configuration = Self {
            authorization_url: parse_url("Authorization URL", &input.authorization_url)?,
            token_url: parse_url("Token URL", &input.token_url)?,
            gateway_url: parse_url("Gateway URL", &input.gateway_url)?,
        };
        configuration.validate()?;
        Ok(configuration)
    }

    pub fn validate(&self) -> Result<(), CommandError> {
        validate_http_url("Authorization URL", &self.authorization_url)?;
        validate_http_url("Token URL", &self.token_url)?;
        validate_gateway_url(&self.gateway_url)
    }

    pub fn authorization_endpoints(&self) -> AuthorizationEndpoints {
        AuthorizationEndpoints {
            authorization_url: self.authorization_url.clone(),
            token_url: self.token_url.clone(),
        }
    }
}

/// User-editable string input for `OpenPrinter` service endpoints.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[allow(clippy::struct_field_names)]
pub struct OpenPrinterServerConfigurationInput {
    pub authorization_url: String,
    pub token_url: String,
    pub gateway_url: String,
}

fn parse_url(field: &str, value: &str) -> Result<Url, CommandError> {
    if value.trim() != value
        || value.is_empty()
        || value.len() > MAX_ENDPOINT_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(invalid_configuration(format!(
            "{field} must be 1 to {MAX_ENDPOINT_BYTES} bytes without surrounding whitespace or control characters.",
        )));
    }

    Url::parse(value)
        .map_err(|_| invalid_configuration(format!("{field} is not a valid absolute URL.")))
}

fn validate_http_url(field: &str, url: &Url) -> Result<(), CommandError> {
    validate_common(field, url)?;
    if url.scheme() != "https" && !(url.scheme() == "http" && is_loopback(url)) {
        return Err(invalid_configuration(format!(
            "{field} must use HTTPS. HTTP is allowed only for a loopback development server.",
        )));
    }
    Ok(())
}

fn validate_gateway_url(url: &Url) -> Result<(), CommandError> {
    validate_common("Gateway URL", url)?;
    if url.scheme() != "wss" && !(url.scheme() == "ws" && is_loopback(url)) {
        return Err(invalid_configuration(
            "Gateway URL must use WSS. WS is allowed only for a loopback development server.",
        ));
    }
    Ok(())
}

fn validate_common(field: &str, url: &Url) -> Result<(), CommandError> {
    if url.as_str().len() > MAX_ENDPOINT_BYTES {
        return Err(invalid_configuration(format!(
            "{field} must not exceed {MAX_ENDPOINT_BYTES} bytes.",
        )));
    }
    if url.cannot_be_a_base() || url.host().is_none() {
        return Err(invalid_configuration(format!(
            "{field} must be an absolute network URL with a host.",
        )));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(invalid_configuration(format!(
            "{field} must not contain embedded credentials.",
        )));
    }
    if url.fragment().is_some() {
        return Err(invalid_configuration(format!(
            "{field} must not contain a URL fragment.",
        )));
    }
    Ok(())
}

fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(address)) => IpAddr::V4(address).is_loopback(),
        Some(Host::Ipv6(address)) => IpAddr::V6(address).is_loopback(),
        None => false,
    }
}

fn invalid_configuration(message: impl AsRef<str>) -> CommandError {
    CommandError::new("invalid_server_configuration", message)
}

#[cfg(test)]
mod tests {
    use url::Url;

    use super::{OpenPrinterServerConfiguration, OpenPrinterServerConfigurationInput};

    fn input(
        authorization_url: &str,
        token_url: &str,
        gateway_url: &str,
    ) -> OpenPrinterServerConfigurationInput {
        OpenPrinterServerConfigurationInput {
            authorization_url: authorization_url.to_owned(),
            token_url: token_url.to_owned(),
            gateway_url: gateway_url.to_owned(),
        }
    }

    #[test]
    fn accepts_tls_and_loopback_development_endpoints() {
        assert!(
            OpenPrinterServerConfiguration::from_input(&input(
                "https://print.example.com/authorize",
                "https://print.example.com/token",
                "wss://print.example.com/openprinter/agent",
            ))
            .is_ok()
        );
        assert!(
            OpenPrinterServerConfiguration::from_input(&input(
                "http://127.0.0.1:8787/authorize",
                "http://localhost:8787/token",
                "ws://[::1]:8787/openprinter/agent",
            ))
            .is_ok()
        );
    }

    #[test]
    fn rejects_insecure_remote_or_credential_bearing_endpoints() {
        assert!(
            OpenPrinterServerConfiguration::from_input(&input(
                "http://print.example.com/authorize",
                "https://print.example.com/token",
                "wss://print.example.com/openprinter/agent",
            ))
            .is_err()
        );
        assert!(
            OpenPrinterServerConfiguration::from_input(&input(
                "https://print.example.com/authorize",
                "https://user:secret@print.example.com/token",
                "wss://print.example.com/openprinter/agent",
            ))
            .is_err()
        );
    }

    #[test]
    fn persisted_configuration_reapplies_endpoint_length_limits() {
        let oversized_path = "a".repeat(2_048);
        let configuration = OpenPrinterServerConfiguration {
            authorization_url: Url::parse(&format!("https://print.example.com/{oversized_path}"))
                .expect("test authorization URL should parse"),
            token_url: Url::parse("https://print.example.com/token")
                .expect("test token URL should parse"),
            gateway_url: Url::parse("wss://print.example.com/openprinter/agent")
                .expect("test gateway URL should parse"),
        };

        assert!(configuration.validate().is_err());
    }
}
