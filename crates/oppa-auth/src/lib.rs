//! OpenPrinter discovery, pairing, and client-owned Ed25519 credentials.
//!
//! Private keys are generated and used only in this Rust layer. The server
//! receives the public JWK during pairing and normal gateway payloads are not
//! individually signed.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::{fmt, sync::Arc, time::Duration};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signer as _, SigningKey};
use oppa_platform::{CredentialStore, PlatformError, SecretValue};
use oppa_protocol::{
    DISCOVERY_PATH, DiscoveryDocument, GatewayAuthenticationChallenge, PairingRequest,
    PairingResponse, PublicEd25519Jwk, Validate,
};
use rand::TryRngCore as _;
use serde::Deserialize;
use thiserror::Error;
use url::Url;
use uuid::Uuid;
use zeroize::Zeroizing;

const MAX_HTTP_RESPONSE_BYTES: usize = 64 * 1024;
const MAX_CHALLENGE_BYTES: usize = 4 * 1024;

/// Public result of generating and securely storing one credential.
#[derive(Clone)]
pub struct GeneratedCredential {
    /// Opaque reference used for later signing and deletion.
    pub credential_ref: String,
    /// Public key safe to send to the OpenPrinter server.
    pub public_key: PublicEd25519Jwk,
}

impl fmt::Debug for GeneratedCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GeneratedCredential")
            .field("credential_ref", &self.credential_ref)
            .field("public_key", &self.public_key)
            .finish_non_exhaustive()
    }
}

/// Ed25519 key lifecycle backed by the platform credential store.
#[derive(Clone)]
pub struct AgentKeyManager {
    store: Arc<dyn CredentialStore>,
}

impl AgentKeyManager {
    /// Creates a key manager over an operating-system or test credential store.
    #[must_use]
    pub fn new(store: Arc<dyn CredentialStore>) -> Self {
        Self { store }
    }

    /// Generates an Ed25519 key, stores its private seed, and returns only its public JWK.
    pub async fn generate(&self, server_id: &str) -> AuthResult<GeneratedCredential> {
        validate_identifier(server_id, "server ID")?;
        let mut seed = Zeroizing::new([0_u8; 32]);
        rand::rngs::OsRng
            .try_fill_bytes(seed.as_mut())
            .map_err(|_| {
                AuthError::Crypto("operating-system randomness is unavailable".to_owned())
            })?;
        let signing_key = SigningKey::from_bytes(&seed);
        let credential_ref = format!("oppa-ed25519-{}", Uuid::new_v4());
        let serialized_seed = Zeroizing::new(signing_key.to_bytes());
        self.store
            .set(
                &credential_ref,
                SecretValue::new(URL_SAFE_NO_PAD.encode(serialized_seed.as_slice())),
            )
            .await?;
        Ok(GeneratedCredential {
            credential_ref,
            public_key: PublicEd25519Jwk {
                kty: "OKP".to_owned(),
                crv: "Ed25519".to_owned(),
                x: URL_SAFE_NO_PAD.encode(signing_key.verifying_key().to_bytes()),
            },
        })
    }

    /// Signs the exact opaque challenge payload bytes.
    pub async fn sign(&self, credential_ref: &str, payload: &[u8]) -> AuthResult<Vec<u8>> {
        validate_credential_ref(credential_ref)?;
        if payload.is_empty() || payload.len() > MAX_CHALLENGE_BYTES {
            return Err(AuthError::InvalidInput(format!(
                "challenge payload must contain 1 to {MAX_CHALLENGE_BYTES} bytes"
            )));
        }
        let secret = self
            .store
            .get(credential_ref)
            .await?
            .ok_or(AuthError::CredentialNotFound)?;
        let decoded = Zeroizing::new(
            URL_SAFE_NO_PAD
                .decode(secret.expose_secret())
                .map_err(|_| AuthError::CredentialEncoding)?,
        );
        let seed = Zeroizing::new(
            <[u8; 32]>::try_from(decoded.as_slice()).map_err(|_| AuthError::CredentialEncoding)?,
        );
        let signing_key = SigningKey::from_bytes(&seed);
        Ok(signing_key.sign(payload).to_bytes().to_vec())
    }

    /// Deletes a private credential. Deleting a missing reference is idempotent.
    pub async fn delete(&self, credential_ref: &str) -> AuthResult<()> {
        validate_credential_ref(credential_ref)?;
        self.store.delete(credential_ref).await?;
        Ok(())
    }

    /// Checks whether a credential reference still resolves in secure storage.
    pub async fn exists(&self, credential_ref: &str) -> AuthResult<bool> {
        validate_credential_ref(credential_ref)?;
        Ok(self.store.get(credential_ref).await?.is_some())
    }

    /// Decodes and signs the exact opaque payload from an authentication challenge.
    pub async fn sign_challenge(
        &self,
        credential_ref: &str,
        challenge: &GatewayAuthenticationChallenge,
    ) -> AuthResult<String> {
        challenge
            .validate()
            .map_err(|error| AuthError::InvalidChallenge(error.to_string()))?;
        let payload = URL_SAFE_NO_PAD.decode(&challenge.payload).map_err(|_| {
            AuthError::InvalidChallenge("payload is not unpadded base64url".to_owned())
        })?;
        let signature = self.sign(credential_ref, &payload).await?;
        Ok(URL_SAFE_NO_PAD.encode(signature))
    }
}

/// Validated discovery result with resolved HTTP and WebSocket endpoints.
#[derive(Debug, Clone)]
pub struct DiscoveredServer {
    /// Validated discovery document.
    pub document: DiscoveryDocument,
    /// Absolute HTTP(S) pairing endpoint.
    pub pairing_url: Url,
    /// Absolute WS(S) gateway endpoint.
    pub gateway_url: Url,
}

/// Bounded HTTP client for discovery and pairing.
#[derive(Clone)]
pub struct PairingClient {
    client: reqwest::Client,
}

impl PairingClient {
    /// Creates an HTTP client with an explicit request timeout.
    pub fn new(timeout: Duration) -> AuthResult<Self> {
        if timeout.is_zero() || timeout > Duration::from_secs(120) {
            return Err(AuthError::InvalidInput(
                "HTTP timeout must be between 1 nanosecond and 120 seconds".to_owned(),
            ));
        }
        let client = reqwest::Client::builder().timeout(timeout).build()?;
        Ok(Self { client })
    }

    /// Fetches current discovery metadata from a normalized server base URL.
    pub async fn discover(&self, server_url: &Url) -> AuthResult<DiscoveredServer> {
        validate_server_url(server_url)?;
        // Build the discovery URL by appending the path relative to the server URL's own path.
        // Using `Url::join` with an absolute path (starting with `/`) would discard the
        // server URL's path segments, turning e.g. `.../api/v1/openprinter/{id}` into just
        // the origin, which breaks servers mounted at a sub-path.
        let mut discovery_url = server_url.clone();
        let base_path = server_url.path().trim_end_matches('/');
        discovery_url.set_path(&format!("{base_path}{DISCOVERY_PATH}"));
        let response = self.client.get(discovery_url.clone()).send().await?;
        let status = response.status();
        let bytes = bounded_body(response).await?;
        if !status.is_success() {
            return Err(AuthError::Discovery(format!(
                "server returned HTTP {status} for url {discovery_url}"
            )));
        }
        let document: DiscoveryDocument = serde_json::from_slice(&bytes).map_err(|error| {
            AuthError::Discovery(format!("invalid discovery response: {error}"))
        })?;
        document
            .validate()
            .map_err(|error| AuthError::Discovery(error.to_string()))?;
        let pairing_url = resolve_endpoint(server_url, &document.endpoints.pairing, false)?;
        let gateway_url = resolve_endpoint(server_url, &document.endpoints.gateway, true)?;
        Ok(DiscoveredServer {
            document,
            pairing_url,
            gateway_url,
        })
    }

    /// Redeems a pairing code and public key at the discovered pairing endpoint.
    pub async fn pair(
        &self,
        discovered: &DiscoveredServer,
        request: &PairingRequest,
    ) -> AuthResult<PairingResponse> {
        request
            .validate()
            .map_err(|error| AuthError::InvalidInput(error.to_string()))?;
        let response = self
            .client
            .post(discovered.pairing_url.clone())
            .json(request)
            .send()
            .await?;
        let status = response.status();
        let bytes = bounded_body(response).await?;
        if status.is_success() {
            let paired: PairingResponse = serde_json::from_slice(&bytes).map_err(|error| {
                AuthError::Pairing(format!("invalid pairing response: {error}"))
            })?;
            paired
                .validate()
                .map_err(|error| AuthError::Pairing(error.to_string()))?;
            if paired.server_id != discovered.document.server.id {
                return Err(AuthError::ServerIdentityChanged);
            }
            return Ok(paired);
        }
        let envelope = serde_json::from_slice::<ErrorEnvelope>(&bytes).ok();
        let safe = envelope.map_or_else(
            || format!("pairing failed with HTTP {status}"),
            |value| format!("{}: {}", value.error.code, value.error.message),
        );
        Err(AuthError::Pairing(safe))
    }
}

#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    code: String,
    message: String,
}

async fn bounded_body(response: reqwest::Response) -> AuthResult<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_HTTP_RESPONSE_BYTES as u64)
    {
        return Err(AuthError::ResponseTooLarge);
    }
    let bytes = response.bytes().await?;
    if bytes.len() > MAX_HTTP_RESPONSE_BYTES {
        return Err(AuthError::ResponseTooLarge);
    }
    Ok(bytes.to_vec())
}

fn resolve_endpoint(base: &Url, value: &str, gateway: bool) -> AuthResult<Url> {
    if value.is_empty() || value.len() > 2_048 || value.chars().any(char::is_whitespace) {
        return Err(AuthError::Discovery(
            "discovery endpoint is invalid".to_owned(),
        ));
    }
    let mut endpoint = if value.starts_with('/') {
        base.join(value)?
    } else {
        Url::parse(value)
            .map_err(|_| AuthError::Discovery("discovery endpoint is not absolute".to_owned()))?
    };
    if gateway {
        match endpoint.scheme() {
            "http" => endpoint
                .set_scheme("ws")
                .map_err(|()| AuthError::Discovery("gateway scheme is invalid".to_owned()))?,
            "https" => endpoint
                .set_scheme("wss")
                .map_err(|()| AuthError::Discovery("gateway scheme is invalid".to_owned()))?,
            "ws" | "wss" => {}
            _ => {
                return Err(AuthError::Discovery(
                    "gateway endpoint must use HTTP(S) or WS(S)".to_owned(),
                ));
            }
        }
    } else if !matches!(endpoint.scheme(), "http" | "https") {
        return Err(AuthError::Discovery(
            "pairing endpoint must use HTTP(S)".to_owned(),
        ));
    }
    if !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.fragment().is_some()
    {
        return Err(AuthError::Discovery(
            "discovery endpoint must not contain credentials or a fragment".to_owned(),
        ));
    }
    let insecure = matches!(endpoint.scheme(), "http" | "ws");
    if insecure && !is_loopback_url(&endpoint) {
        return Err(AuthError::Discovery(
            "discovery endpoint must use TLS outside loopback development".to_owned(),
        ));
    }
    Ok(endpoint)
}

/// Validates and normalizes an HTTP(S) OpenPrinter server base URL.
pub fn normalize_server_url(value: &str) -> AuthResult<Url> {
    if value.trim() != value
        || value.is_empty()
        || value.len() > 2_048
        || value.chars().any(char::is_control)
    {
        return Err(AuthError::InvalidInput("server URL is invalid".to_owned()));
    }
    let mut url = Url::parse(value)
        .map_err(|_| AuthError::InvalidInput("server URL must be absolute".to_owned()))?;
    validate_server_url(&url)?;
    url.set_query(None);
    url.set_fragment(None);
    let normalized_path = url.path().trim_end_matches('/').to_owned();
    url.set_path(&normalized_path);
    if url.path().is_empty() {
        url.set_path("/");
    }
    Ok(url)
}

fn validate_server_url(url: &Url) -> AuthResult<()> {
    let loopback = is_loopback_url(url);
    if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
        return Err(AuthError::InvalidInput(
            "server URL must use HTTPS; HTTP is allowed only for loopback development".to_owned(),
        ));
    }
    if url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(AuthError::InvalidInput(
            "server URL must have a host and must not contain credentials or a fragment".to_owned(),
        ));
    }
    Ok(())
}

fn is_loopback_url(url: &Url) -> bool {
    url.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    })
}

fn validate_identifier(value: &str, name: &str) -> AuthResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
    {
        return Err(AuthError::InvalidInput(format!(
            "{name} is not a valid OpenPrinter identifier"
        )));
    }
    Ok(())
}

fn validate_credential_ref(value: &str) -> AuthResult<()> {
    if !value.starts_with("oppa-ed25519-")
        || value.len() > 128
        || value.chars().any(char::is_control)
    {
        return Err(AuthError::InvalidInput(
            "credential reference is invalid".to_owned(),
        ));
    }
    Ok(())
}

/// Expected discovery, pairing, key-management, and signing failures.
#[derive(Debug, Error)]
pub enum AuthError {
    /// Input was rejected before network or secure-store access.
    #[error("invalid authentication input: {0}")]
    InvalidInput(String),
    /// Discovery failed or advertised unsupported metadata.
    #[error("server discovery failed: {0}")]
    Discovery(String),
    /// Pairing was rejected or returned an invalid response.
    #[error("pairing failed: {0}")]
    Pairing(String),
    /// A paired URL returned a different server identity.
    #[error(
        "the configured URL now identifies a different OpenPrinter server; pair again explicitly"
    )]
    ServerIdentityChanged,
    /// Credential reference did not exist in secure storage.
    #[error("the local agent credential was not found")]
    CredentialNotFound,
    /// Stored credential bytes were malformed.
    #[error("the local agent credential could not be decoded")]
    CredentialEncoding,
    /// Challenge payload was invalid.
    #[error("gateway challenge is invalid: {0}")]
    InvalidChallenge(String),
    /// Cryptographic operation could not be completed.
    #[error("Ed25519 operation failed: {0}")]
    Crypto(String),
    /// HTTP response exceeded the fixed limit.
    #[error("server response exceeded the authentication size limit")]
    ResponseTooLarge,
    /// Secure platform storage failed.
    #[error(transparent)]
    Platform(#[from] PlatformError),
    /// HTTP request failed.
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    /// URL parsing or endpoint resolution failed.
    #[error(transparent)]
    Url(#[from] url::ParseError),
}

/// Result alias for authentication operations.
pub type AuthResult<T> = Result<T, AuthError>;

#[cfg(test)]
mod tests {
    use ed25519_dalek::{Signature, Verifier as _, VerifyingKey};
    use oppa_platform::MemoryCredentialStore;

    use super::*;

    #[tokio::test]
    async fn generated_key_signs_without_exposing_private_material() {
        let manager = AgentKeyManager::new(Arc::new(MemoryCredentialStore::default()));
        let generated = manager.generate("server-01").await.expect("generate");
        let payload = b"openprinter-auth-v1\nopaque";
        let signature = manager
            .sign(&generated.credential_ref, payload)
            .await
            .expect("sign");
        let public: [u8; 32] = URL_SAFE_NO_PAD
            .decode(&generated.public_key.x)
            .expect("base64")
            .try_into()
            .expect("size");
        VerifyingKey::from_bytes(&public)
            .expect("public key")
            .verify(
                payload,
                &Signature::from_slice(&signature).expect("signature"),
            )
            .expect("compatible signature");
        assert!(
            manager
                .exists(&generated.credential_ref)
                .await
                .expect("exists")
        );
        manager
            .delete(&generated.credential_ref)
            .await
            .expect("delete");
        assert!(
            !manager
                .exists(&generated.credential_ref)
                .await
                .expect("missing")
        );
        assert!(!format!("{generated:?}").contains(secret_marker()));
    }

    #[test]
    fn discovery_url_preserves_server_url_path_prefix() {
        // Servers mounted under a sub-path (e.g. /api/v1/openprinter/{id}) must have
        // the discovery path appended to that prefix, not resolved from the host root.
        let path_url = normalize_server_url("https://api.example.com/api/v1/openprinter/71217dfb")
            .expect("path URL");
        let mut discovery = path_url.clone();
        let base_path = path_url.path().trim_end_matches('/');
        discovery.set_path(&format!("{base_path}{DISCOVERY_PATH}"));
        assert_eq!(
            discovery.as_str(),
            "https://api.example.com/api/v1/openprinter/71217dfb/.well-known/openprinter"
        );

        // Root-only URLs should still resolve to /.well-known/openprinter.
        let root_url = normalize_server_url("https://print.example.com/").expect("root URL");
        let mut discovery_root = root_url.clone();
        let base_path_root = root_url.path().trim_end_matches('/');
        discovery_root.set_path(&format!("{base_path_root}{DISCOVERY_PATH}"));
        assert_eq!(
            discovery_root.as_str(),
            "https://print.example.com/.well-known/openprinter"
        );
    }

    #[test]
    fn server_url_requires_tls_outside_loopback_and_normalizes_slashes() {
        assert_eq!(
            normalize_server_url("http://127.0.0.1:8787/")
                .expect("loopback")
                .as_str(),
            "http://127.0.0.1:8787/"
        );
        assert_eq!(
            normalize_server_url("https://print.example.com///")
                .expect("tls")
                .as_str(),
            "https://print.example.com/"
        );
        assert!(normalize_server_url("http://print.example.com").is_err());
        assert!(normalize_server_url("https://user:secret@print.example.com").is_err());
    }

    #[test]
    fn discovered_endpoints_cannot_downgrade_remote_transport_security() {
        let base = Url::parse("https://print.example.com/").expect("base URL");
        assert!(resolve_endpoint(&base, "http://print.example.com/pair", false).is_err());
        assert!(resolve_endpoint(&base, "ws://print.example.com/gateway", true).is_err());
        assert_eq!(
            resolve_endpoint(&base, "/gateway", true)
                .expect("relative secure gateway")
                .scheme(),
            "wss"
        );
        let loopback = Url::parse("http://127.0.0.1:8787/").expect("loopback URL");
        assert!(resolve_endpoint(&loopback, "/gateway", true).is_ok());
    }

    fn secret_marker() -> &'static str {
        "private-key-marker-never-present"
    }
}
