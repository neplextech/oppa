use oppa_auth::normalize_server_url;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

use crate::error::CommandError;

pub const SERVER_CONFIGURATION_SETTING: &str = "openprinter-server-configuration-v2";
pub const LEGACY_SERVER_CONFIGURATION_SETTING: &str = "openprinter-server-configuration-v1";
pub const CONNECTION_SETTING: &str = "openprinter-connection-v1";
pub const DEFAULT_SERVER_URL: &str = "http://127.0.0.1:8787/";

/// Validated, non-secret `OpenPrinter` base URL persisted by the desktop host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenPrinterServerConfiguration {
    pub server_url: Url,
}

impl Default for OpenPrinterServerConfiguration {
    fn default() -> Self {
        Self {
            server_url: Url::parse(DEFAULT_SERVER_URL).expect("built-in server URL is valid"),
        }
    }
}

impl OpenPrinterServerConfiguration {
    pub fn from_input(input: &OpenPrinterServerConfigurationInput) -> Result<Self, CommandError> {
        let server_url = normalize_server_url(&input.server_url)
            .map_err(|error| invalid_configuration(error.to_string()))?;
        Ok(Self { server_url })
    }

    pub fn validate(&self) -> Result<(), CommandError> {
        normalize_server_url(self.server_url.as_str())
            .map(|_| ())
            .map_err(|error| invalid_configuration(error.to_string()))
    }
}

/// User-editable server base URL.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenPrinterServerConfigurationInput {
    pub server_url: String,
}

/// Non-secret identity and secure-key reference saved after pairing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenPrinterConnection {
    pub server_url: Url,
    pub server_id: String,
    pub agent_id: String,
    pub key_id: String,
    pub credential_ref: String,
}

/// Result of examining a legacy three-endpoint configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyConfigurationMigration {
    pub suggested: Option<OpenPrinterServerConfiguration>,
    pub requires_pairing: bool,
}

/// Derives a suggested base URL only when all legacy endpoints share an origin.
#[must_use]
pub fn migrate_legacy_configuration(value: &Value) -> Option<LegacyConfigurationMigration> {
    let object = value.as_object()?;
    let authorization = Url::parse(object.get("authorizationUrl")?.as_str()?).ok()?;
    let token = Url::parse(object.get("tokenUrl")?.as_str()?).ok()?;
    let gateway = Url::parse(object.get("gatewayUrl")?.as_str()?).ok()?;
    let authorization_origin = normalized_origin(&authorization)?;
    let token_origin = normalized_origin(&token)?;
    let gateway_origin = normalized_origin(&gateway)?;
    let suggested = if authorization_origin == token_origin && token_origin == gateway_origin {
        let server_url = normalize_server_url(&authorization_origin).ok()?;
        Some(OpenPrinterServerConfiguration { server_url })
    } else {
        None
    };
    Some(LegacyConfigurationMigration {
        suggested,
        requires_pairing: true,
    })
}

fn normalized_origin(url: &Url) -> Option<String> {
    let scheme = match url.scheme() {
        "http" | "ws" => "http",
        "https" | "wss" => "https",
        _ => return None,
    };
    let host = url.host_str()?;
    let port = url.port();
    Some(match port {
        Some(port) => format!("{scheme}://{host}:{port}/"),
        None => format!("{scheme}://{host}/"),
    })
}

fn invalid_configuration(message: impl AsRef<str>) -> CommandError {
    CommandError::new("invalid_server_configuration", message)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn accepts_tls_and_loopback_and_normalizes_trailing_slashes() {
        let remote =
            OpenPrinterServerConfiguration::from_input(&OpenPrinterServerConfigurationInput {
                server_url: "https://print.example.com///".to_owned(),
            })
            .expect("remote TLS");
        assert_eq!(remote.server_url.as_str(), "https://print.example.com/");
        assert!(
            OpenPrinterServerConfiguration::from_input(&OpenPrinterServerConfigurationInput {
                server_url: "http://127.0.0.1:8787".to_owned(),
            })
            .is_ok()
        );
        assert!(
            OpenPrinterServerConfiguration::from_input(&OpenPrinterServerConfigurationInput {
                server_url: "http://print.example.com".to_owned(),
            })
            .is_err()
        );
    }

    #[test]
    fn migration_suggests_only_a_shared_legacy_origin() {
        let shared = json!({
            "authorizationUrl": "http://127.0.0.1:8787/authorize",
            "tokenUrl": "http://127.0.0.1:8787/token",
            "gatewayUrl": "ws://127.0.0.1:8787/openprinter/agent"
        });
        let migration = migrate_legacy_configuration(&shared).expect("legacy");
        assert_eq!(
            migration.suggested.expect("suggested").server_url.as_str(),
            DEFAULT_SERVER_URL
        );
        assert!(migration.requires_pairing);

        let split = json!({
            "authorizationUrl": "https://accounts.example.com/authorize",
            "tokenUrl": "https://accounts.example.com/token",
            "gatewayUrl": "wss://gateway.example.com/openprinter"
        });
        assert!(
            migrate_legacy_configuration(&split)
                .expect("legacy")
                .suggested
                .is_none()
        );
    }

    #[test]
    fn paired_connection_serialization_contains_only_non_secret_metadata() {
        let connection = OpenPrinterConnection {
            server_url: Url::parse("https://print.example.com/").expect("URL"),
            server_id: "server-01".to_owned(),
            agent_id: "agent-01".to_owned(),
            key_id: "key-01".to_owned(),
            credential_ref: "oppa-ed25519-reference".to_owned(),
        };
        let serialized = serde_json::to_value(connection).expect("serialize connection");
        assert_eq!(serialized["credentialRef"], "oppa-ed25519-reference");
        assert!(serialized.get("privateKey").is_none());
        assert!(serialized.get("publicKey").is_none());
        assert!(serialized.get("signature").is_none());
    }
}
