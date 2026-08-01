use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};

use crate::{
    PROTOCOL_VERSION, Validate, ValidationError,
    validation::{validate_brand_name, validate_identifier, validate_string, validate_timestamp},
};

/// Discovery authentication method implemented by protocol version 1.
pub const AUTHENTICATION_METHOD: &str = "pairing-code-ed25519";
/// Signature algorithm implemented by protocol version 1.
pub const SIGNATURE_ALGORITHM: &str = "Ed25519";
/// Default discovery endpoint.
pub const DISCOVERY_PATH: &str = "/.well-known/openprinter";

/// Public subset of an Ed25519 JSON Web Key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicEd25519Jwk {
    /// Key type, always `OKP`.
    pub kty: String,
    /// Curve, always `Ed25519`.
    pub crv: String,
    /// Unpadded base64url 32-byte public key.
    pub x: String,
}

impl Validate for PublicEd25519Jwk {
    fn validate(&self) -> Result<(), ValidationError> {
        if self.kty != "OKP" || self.crv != "Ed25519" {
            return Err(ValidationError::new(
                "publicKey",
                "must be an Ed25519 OKP JWK",
            ));
        }
        let decoded = URL_SAFE_NO_PAD
            .decode(&self.x)
            .map_err(|_| ValidationError::new("publicKey.x", "must be unpadded base64url"))?;
        if decoded.len() != 32 || URL_SAFE_NO_PAD.encode(&decoded) != self.x {
            return Err(ValidationError::new(
                "publicKey.x",
                "must encode exactly 32 bytes",
            ));
        }
        Ok(())
    }
}

/// Human-readable server identity returned by discovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoveryServer {
    /// Stable server identity.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Server implementation version.
    pub version: String,
}

/// Pairing and gateway endpoints returned by discovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoveryEndpoints {
    /// Relative or absolute pairing endpoint.
    pub pairing: String,
    /// Relative or absolute WebSocket gateway endpoint.
    pub gateway: String,
}

/// Authentication policy returned by discovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoveryAuthentication {
    /// Authentication method discriminator.
    pub method: String,
    /// Challenge lifetime in whole seconds.
    pub challenge_ttl_seconds: u32,
}

/// Discovery document returned by an OpenPrinter server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoveryDocument {
    /// Protocol version, distinct from the server version.
    pub protocol_version: String,
    /// Server identity.
    pub server: DiscoveryServer,
    /// Current endpoint locations.
    pub endpoints: DiscoveryEndpoints,
    /// Authentication policy.
    pub authentication: DiscoveryAuthentication,
}

impl Validate for DiscoveryDocument {
    fn validate(&self) -> Result<(), ValidationError> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(ValidationError::new("protocolVersion", "is unsupported"));
        }
        if self.authentication.method != AUTHENTICATION_METHOD {
            return Err(ValidationError::new(
                "authentication.method",
                "is unsupported",
            ));
        }
        validate_identifier(&self.server.id, "server.id", 128)?;
        validate_brand_name(&self.server.name, "server.name")?;
        validate_string(&self.server.version, "server.version", 1, 256)?;
        validate_endpoint(&self.endpoints.pairing, "endpoints.pairing")?;
        validate_endpoint(&self.endpoints.gateway, "endpoints.gateway")?;
        if !(5..=300).contains(&self.authentication.challenge_ttl_seconds) {
            return Err(ValidationError::new(
                "authentication.challengeTtlSeconds",
                "must be between 5 and 300",
            ));
        }
        Ok(())
    }
}

fn validate_endpoint(value: &str, field: &'static str) -> Result<(), ValidationError> {
    validate_string(value, field, 1, 2048)?;
    if value.chars().any(char::is_whitespace)
        || !(value.starts_with('/')
            || value.starts_with("http://")
            || value.starts_with("https://")
            || value.starts_with("ws://")
            || value.starts_with("wss://"))
    {
        return Err(ValidationError::new(
            field,
            "must be a relative or absolute OpenPrinter endpoint",
        ));
    }
    Ok(())
}

/// Agent metadata submitted during pairing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PairingAgent {
    /// User-visible local agent name.
    pub name: String,
    /// Running agent version.
    pub version: String,
    /// Operating-system identifier.
    pub platform: String,
    /// Stable local installation identifier.
    pub installation_id: String,
}

/// Public credential submitted during pairing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PairingCredential {
    /// Signature algorithm.
    pub algorithm: String,
    /// Public key; the private key never enters this request.
    pub public_key: PublicEd25519Jwk,
}

/// Pairing request sent after local key generation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PairingRequest {
    /// Protocol version.
    pub protocol_version: String,
    /// Temporary pairing code.
    pub code: String,
    /// Bounded local agent metadata.
    pub agent: PairingAgent,
    /// Client-generated public credential.
    pub credential: PairingCredential,
}

impl Validate for PairingRequest {
    fn validate(&self) -> Result<(), ValidationError> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(ValidationError::new("protocolVersion", "is unsupported"));
        }
        if !(4..=64).contains(&self.code.len())
            || !self
                .code
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(ValidationError::new(
                "code",
                "must be a bounded pairing code",
            ));
        }
        validate_string(&self.agent.name, "agent.name", 1, 128)?;
        validate_string(&self.agent.version, "agent.version", 1, 256)?;
        validate_string(&self.agent.platform, "agent.platform", 1, 64)?;
        validate_identifier(&self.agent.installation_id, "agent.installationId", 128)?;
        if self.credential.algorithm != SIGNATURE_ALGORITHM {
            return Err(ValidationError::new(
                "credential.algorithm",
                "is unsupported",
            ));
        }
        self.credential.public_key.validate()
    }
}

/// Successful non-secret pairing response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PairingResponse {
    /// Server-assigned agent identity.
    pub agent_id: String,
    /// Server-assigned credential identity.
    pub key_id: String,
    /// Discovered server identity.
    pub server_id: String,
    /// UTC pairing timestamp.
    pub paired_at: String,
}

impl Validate for PairingResponse {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_identifier(&self.agent_id, "agentId", 128)?;
        validate_identifier(&self.key_id, "keyId", 128)?;
        validate_identifier(&self.server_id, "serverId", 128)?;
        validate_timestamp(&self.paired_at, "pairedAt")
    }
}

/// Socket-bound gateway challenge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GatewayAuthenticationChallenge {
    /// Discriminator, always `auth.challenge`.
    #[serde(rename = "type")]
    pub message_type: String,
    /// Single-use challenge identifier.
    pub challenge_id: String,
    /// Opaque base64url bytes to sign exactly.
    pub payload: String,
    /// Challenge expiration timestamp.
    pub expires_at: String,
}

impl Validate for GatewayAuthenticationChallenge {
    fn validate(&self) -> Result<(), ValidationError> {
        if self.message_type != "auth.challenge" {
            return Err(ValidationError::new("type", "must be auth.challenge"));
        }
        validate_identifier(&self.challenge_id, "challengeId", 128)?;
        validate_timestamp(&self.expires_at, "expiresAt")?;
        validate_base64url(&self.payload, "payload", 1, 4096, None)
    }
}

/// Successful gateway authentication result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GatewayAuthenticationAccepted {
    /// Discriminator, always `auth.accepted`.
    #[serde(rename = "type")]
    pub message_type: String,
    /// Authenticated session identity.
    pub session_id: String,
    /// Authenticated agent identity.
    pub agent_id: String,
    /// Expected heartbeat interval.
    pub heartbeat_interval_ms: u32,
}

impl Validate for GatewayAuthenticationAccepted {
    fn validate(&self) -> Result<(), ValidationError> {
        if self.message_type != "auth.accepted" {
            return Err(ValidationError::new("type", "must be auth.accepted"));
        }
        validate_identifier(&self.session_id, "sessionId", 128)?;
        validate_identifier(&self.agent_id, "agentId", 128)?;
        if !(5_000..=300_000).contains(&self.heartbeat_interval_ms) {
            return Err(ValidationError::new(
                "heartbeatIntervalMs",
                "must be between 5000 and 300000",
            ));
        }
        Ok(())
    }
}

/// Rejected gateway authentication result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GatewayAuthenticationRejected {
    /// Discriminator, always `auth.rejected`.
    #[serde(rename = "type")]
    pub message_type: String,
    /// Stable failure code.
    pub code: String,
    /// Bounded safe message.
    pub message: String,
}

impl Validate for GatewayAuthenticationRejected {
    fn validate(&self) -> Result<(), ValidationError> {
        const CODES: &[&str] = &[
            "challenge_expired",
            "challenge_invalid",
            "challenge_consumed",
            "credential_not_found",
            "credential_revoked",
            "invalid_signature",
            "unsupported_algorithm",
            "authentication_timeout",
        ];
        if self.message_type != "auth.rejected" {
            return Err(ValidationError::new("type", "must be auth.rejected"));
        }
        if !CODES.contains(&self.code.as_str()) {
            return Err(ValidationError::new(
                "code",
                "is not a supported authentication failure",
            ));
        }
        validate_string(&self.message, "message", 1, 512)
    }
}

/// Server authentication frame received before normal protocol traffic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GatewayAuthenticationServerMessage {
    /// Initial challenge.
    Challenge(GatewayAuthenticationChallenge),
    /// Successful proof result.
    Accepted(GatewayAuthenticationAccepted),
    /// Failed proof result.
    Rejected(GatewayAuthenticationRejected),
}

impl Validate for GatewayAuthenticationServerMessage {
    fn validate(&self) -> Result<(), ValidationError> {
        match self {
            Self::Challenge(value) => value.validate(),
            Self::Accepted(value) => value.validate(),
            Self::Rejected(value) => value.validate(),
        }
    }
}

/// Client signature proof for one challenge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GatewayAuthenticationResponse {
    /// Discriminator, always `auth.response`.
    #[serde(rename = "type")]
    pub message_type: String,
    /// Challenge identifier copied exactly.
    pub challenge_id: String,
    /// Paired agent identity.
    pub agent_id: String,
    /// Paired credential identity.
    pub key_id: String,
    /// Signature algorithm.
    pub algorithm: String,
    /// Unpadded base64url Ed25519 signature.
    pub signature: String,
}

impl Validate for GatewayAuthenticationResponse {
    fn validate(&self) -> Result<(), ValidationError> {
        if self.message_type != "auth.response" {
            return Err(ValidationError::new("type", "must be auth.response"));
        }
        validate_identifier(&self.challenge_id, "challengeId", 128)?;
        validate_identifier(&self.agent_id, "agentId", 128)?;
        validate_identifier(&self.key_id, "keyId", 128)?;
        if self.algorithm != SIGNATURE_ALGORITHM {
            return Err(ValidationError::new("algorithm", "is unsupported"));
        }
        validate_base64url(&self.signature, "signature", 86, 86, Some(64))
    }
}

fn validate_base64url(
    value: &str,
    field: &'static str,
    minimum: usize,
    maximum: usize,
    decoded_length: Option<usize>,
) -> Result<(), ValidationError> {
    if !(minimum..=maximum).contains(&value.len()) {
        return Err(ValidationError::new(field, "has an invalid encoded length"));
    }
    let decoded = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| ValidationError::new(field, "must be canonical unpadded base64url"))?;
    if URL_SAFE_NO_PAD.encode(&decoded) != value
        || decoded_length.is_some_and(|length| decoded.len() != length)
    {
        return Err(ValidationError::new(
            field,
            "must be canonical unpadded base64url",
        ));
    }
    Ok(())
}
