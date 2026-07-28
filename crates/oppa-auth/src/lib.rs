//! Provider-neutral authorization-code flow for OPPA.
//!
//! The implementation uses PKCE, a high-entropy one-time state value, and an
//! ephemeral callback listener bound exclusively to `127.0.0.1`. The
//! integrating provider remains responsible for login, account selection,
//! approval, code issuance, token issuance, and permission policy.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod loopback;

use std::{fmt, sync::Arc, time::Duration};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
pub use loopback::{AuthorizationCallback, LoopbackCallback};
use oppa_core::AgentId;
use oppa_platform::{BrowserOpener, CredentialStore, PlatformError, SecretValue, SystemBrowser};
use oppa_product::ProductConfig;
use rand::RngCore;
use reqwest::{Client, redirect::Policy};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use thiserror::Error;
use tokio::sync::RwLock;
use url::Url;

/// Default maximum lifetime for one browser callback listener.
pub const DEFAULT_CALLBACK_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// Maximum token response body accepted from a provider.
pub const MAX_TOKEN_RESPONSE_BYTES: usize = 64 * 1024;
const CREDENTIAL_ACCOUNT: &str = "oauth-token-set-v1";

/// OAuth-style provider endpoints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationEndpoints {
    /// Browser authorization endpoint.
    pub authorization_url: Url,
    /// Code and refresh exchange endpoint.
    pub token_url: Url,
}

impl AuthorizationEndpoints {
    /// Copies provider endpoints from immutable product configuration.
    #[must_use]
    pub fn from_product(product: &ProductConfig) -> Self {
        Self {
            authorization_url: product.protocol.authorization_url.clone(),
            token_url: product.protocol.token_url.clone(),
        }
    }

    /// Validates TLS requirements, allowing plaintext only on loopback for
    /// local development.
    pub fn validate(&self) -> AuthResult<()> {
        validate_http_endpoint("authorization URL", &self.authorization_url)?;
        validate_http_endpoint("token URL", &self.token_url)
    }
}

/// PKCE verifier and its public S256 challenge.
pub struct PkcePair {
    verifier: SecretValue,
    challenge: String,
}

impl fmt::Debug for PkcePair {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PkcePair")
            .field("verifier", &"[REDACTED]")
            .field("challenge", &self.challenge)
            .finish()
    }
}

impl PkcePair {
    /// Generates a cryptographically random RFC 7636 verifier and S256
    /// challenge.
    #[must_use]
    pub fn generate() -> Self {
        let mut entropy = [0_u8; 64];
        rand::rng().fill_bytes(&mut entropy);
        Self::from_verifier(URL_SAFE_NO_PAD.encode(entropy))
    }

    /// Constructs a pair from a verifier after validating RFC 7636 syntax.
    pub fn try_from_verifier(verifier: impl Into<String>) -> AuthResult<Self> {
        let verifier = verifier.into();
        if !(43..=128).contains(&verifier.len())
            || !verifier.bytes().all(
                |byte| matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~'),
            )
        {
            return Err(AuthError::InvalidPkceVerifier);
        }
        Ok(Self::from_verifier(verifier))
    }

    fn from_verifier(verifier: String) -> Self {
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        Self {
            verifier: SecretValue::new(verifier),
            challenge,
        }
    }

    /// Returns the S256 challenge sent to the authorization endpoint.
    #[must_use]
    pub fn challenge(&self) -> &str {
        &self.challenge
    }

    fn verifier(&self) -> &str {
        self.verifier.expose_secret()
    }
}

/// High-entropy one-time state value.
pub struct AuthorizationState(SecretValue);

impl fmt::Debug for AuthorizationState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AuthorizationState([REDACTED])")
    }
}

impl AuthorizationState {
    /// Generates a state value with 256 bits of entropy.
    #[must_use]
    pub fn generate() -> Self {
        let mut entropy = [0_u8; 32];
        rand::rng().fill_bytes(&mut entropy);
        Self(SecretValue::new(URL_SAFE_NO_PAD.encode(entropy)))
    }

    /// Creates a state value for deterministic tests.
    pub fn from_value(value: impl Into<String>) -> AuthResult<Self> {
        let value = value.into();
        if value.len() < 32
            || value.len() > 256
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(AuthError::InvalidState);
        }
        Ok(Self(SecretValue::new(value)))
    }

    /// Compares a callback state without data-dependent early exit.
    #[must_use]
    pub fn matches(&self, candidate: &str) -> bool {
        let expected = self.0.expose_secret().as_bytes();
        let candidate = candidate.as_bytes();
        expected.len() == candidate.len() && bool::from(expected.ct_eq(candidate))
    }

    fn expose(&self) -> &str {
        self.0.expose_secret()
    }
}

/// Browser authorization flow waiting for a one-time loopback callback.
pub struct PendingAuthorization {
    /// URL that must be opened in the user's system browser.
    pub authorization_url: Url,
    callback: LoopbackCallback,
    redirect_uri: Url,
    state: AuthorizationState,
    pkce: PkcePair,
}

impl PendingAuthorization {
    /// Opens this session's authorization URL with a supplied platform
    /// integration.
    pub fn open_browser(&self, browser: &dyn BrowserOpener) -> AuthResult<()> {
        browser
            .open(&self.authorization_url)
            .map_err(AuthError::Platform)
    }

    /// Opens this session using the operating-system URL handler.
    pub fn open_system_browser(&self) -> AuthResult<()> {
        self.open_browser(&SystemBrowser)
    }

    /// Waits for the callback, validates state, and exchanges the code.
    pub async fn complete(self, client: &AuthorizationClient) -> AuthResult<TokenSet> {
        let callback = self.callback.wait(&self.state).await?;
        client
            .exchange_code(
                callback.code.expose_secret(),
                &self.redirect_uri,
                &self.pkce,
            )
            .await
    }

    /// Returns the loopback redirect URI registered for this session.
    #[must_use]
    pub fn redirect_uri(&self) -> &Url {
        &self.redirect_uri
    }
}

/// Public-client authorization and token-exchange implementation.
#[derive(Clone)]
pub struct AuthorizationClient {
    endpoints: AuthorizationEndpoints,
    client_id: String,
    scopes: Vec<String>,
    callback_timeout: Duration,
    http: Client,
}

impl AuthorizationClient {
    /// Creates a public-client implementation with redirects disabled for token
    /// requests.
    pub fn new(
        endpoints: AuthorizationEndpoints,
        client_id: impl Into<String>,
        scopes: Vec<String>,
        request_timeout: Duration,
    ) -> AuthResult<Self> {
        endpoints.validate()?;
        let client_id = client_id.into();
        validate_parameter("client id", &client_id, 256)?;
        for scope in &scopes {
            validate_parameter("scope", scope, 128)?;
            if scope.contains(' ') {
                return Err(AuthError::InvalidConfiguration(
                    "individual scope values must not contain spaces".to_owned(),
                ));
            }
        }
        let http = Client::builder()
            .timeout(request_timeout)
            .redirect(Policy::none())
            .build()
            .map_err(|error| AuthError::Http(error.to_string()))?;
        Ok(Self {
            endpoints,
            client_id,
            scopes,
            callback_timeout: DEFAULT_CALLBACK_TIMEOUT,
            http,
        })
    }

    /// Overrides the callback lifetime, primarily for controlled tests.
    #[must_use]
    pub const fn with_callback_timeout(mut self, timeout: Duration) -> Self {
        self.callback_timeout = timeout;
        self
    }

    /// Binds an ephemeral loopback listener and constructs the browser URL.
    pub async fn begin(&self) -> AuthResult<PendingAuthorization> {
        let callback = LoopbackCallback::bind(self.callback_timeout).await?;
        let redirect_uri = callback.redirect_uri().clone();
        let state = AuthorizationState::generate();
        let pkce = PkcePair::generate();
        let mut authorization_url = self.endpoints.authorization_url.clone();
        {
            let mut query = authorization_url.query_pairs_mut();
            query
                .append_pair("response_type", "code")
                .append_pair("client_id", &self.client_id)
                .append_pair("redirect_uri", redirect_uri.as_str())
                .append_pair("code_challenge", pkce.challenge())
                .append_pair("code_challenge_method", "S256")
                .append_pair("state", state.expose());
            if !self.scopes.is_empty() {
                query.append_pair("scope", &self.scopes.join(" "));
            }
        }
        Ok(PendingAuthorization {
            authorization_url,
            callback,
            redirect_uri,
            state,
            pkce,
        })
    }

    /// Exchanges a one-time authorization code.
    pub async fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &Url,
        pkce: &PkcePair,
    ) -> AuthResult<TokenSet> {
        validate_parameter("authorization code", code, 4096)?;
        let form = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri.as_str()),
            ("client_id", self.client_id.as_str()),
            ("code_verifier", pkce.verifier()),
        ];
        self.exchange(&form, None).await
    }

    /// Exchanges a refresh credential for a new token set.
    ///
    /// When the provider omits refresh-token rotation, the prior credential is
    /// retained.
    pub async fn refresh(&self, current: &TokenSet) -> AuthResult<TokenSet> {
        let refresh = current
            .refresh_token
            .as_ref()
            .ok_or(AuthError::MissingRefreshToken)?;
        let form = [
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh.expose_secret()),
            ("client_id", self.client_id.as_str()),
        ];
        let mut refreshed = self.exchange(&form, Some(&current.agent_id)).await?;
        if refreshed.refresh_token.is_none() {
            refreshed.refresh_token = Some(refresh.clone());
        }
        Ok(refreshed)
    }

    async fn exchange(
        &self,
        form: &[(&str, &str)],
        prior_agent_id: Option<&AgentId>,
    ) -> AuthResult<TokenSet> {
        let mut response = self
            .http
            .post(self.endpoints.token_url.clone())
            .form(form)
            .send()
            .await
            .map_err(|error| AuthError::Http(error.to_string()))?;
        let status = response.status();
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| AuthError::Http(error.to_string()))?
        {
            let new_length = bytes.len().saturating_add(chunk.len());
            if new_length > MAX_TOKEN_RESPONSE_BYTES {
                return Err(AuthError::TokenResponseTooLarge(new_length));
            }
            bytes.extend_from_slice(&chunk);
        }
        if !status.is_success() {
            let provider_error = serde_json::from_slice::<ProviderTokenError>(&bytes)
                .map(|error| {
                    let description = error
                        .error_description
                        .unwrap_or_else(|| "provider rejected token exchange".to_owned());
                    format!("{}: {}", error.error, truncate(&description, 500))
                })
                .unwrap_or_else(|_| format!("provider returned HTTP {status}"));
            return Err(AuthError::Provider(provider_error));
        }
        let response: TokenResponse = serde_json::from_slice(&bytes)
            .map_err(|error| AuthError::InvalidTokenResponse(error.to_string()))?;
        response.into_token_set(prior_agent_id)
    }
}

/// Provider-issued credentials held only in memory or secure platform storage.
pub struct TokenSet {
    /// Stable identity assigned by the provider and used in agent hello.
    pub agent_id: AgentId,
    /// Bearer access credential.
    pub access_token: SecretValue,
    /// Refresh credential, when the provider issues one.
    pub refresh_token: Option<SecretValue>,
    /// Provider token type, currently required to be Bearer.
    pub token_type: String,
    /// Optional absolute expiration time.
    pub expires_at: Option<DateTime<Utc>>,
    /// Provider-granted scope string.
    pub scope: Option<String>,
}

impl fmt::Debug for TokenSet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TokenSet")
            .field("agent_id", &self.agent_id)
            .field("access_token", &"[REDACTED]")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("token_type", &self.token_type)
            .field("expires_at", &self.expires_at)
            .field("scope", &self.scope)
            .finish()
    }
}

impl TokenSet {
    /// Returns whether the access token is expired or will expire within the
    /// supplied safety window.
    #[must_use]
    pub fn expires_within(&self, window: ChronoDuration) -> bool {
        self.expires_at
            .is_some_and(|expires_at| expires_at <= Utc::now() + window)
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    #[serde(default)]
    agent_id: Option<String>,
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    token_type: String,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    scope: Option<String>,
}

impl TokenResponse {
    fn into_token_set(self, prior_agent_id: Option<&AgentId>) -> AuthResult<TokenSet> {
        let agent_id = match self.agent_id {
            Some(agent_id) => AgentId::new(agent_id)
                .map_err(|error| AuthError::InvalidTokenResponse(error.to_string()))?,
            None => prior_agent_id.cloned().ok_or_else(|| {
                AuthError::InvalidTokenResponse(
                    "initial token response must contain agent_id".to_owned(),
                )
            })?,
        };
        validate_token("access token", &self.access_token)?;
        if let Some(refresh) = &self.refresh_token {
            validate_token("refresh token", refresh)?;
        }
        if !self.token_type.eq_ignore_ascii_case("bearer") {
            return Err(AuthError::InvalidTokenResponse(
                "token_type must be Bearer".to_owned(),
            ));
        }
        if self
            .scope
            .as_ref()
            .is_some_and(|scope| scope.len() > 4096 || scope.chars().any(char::is_control))
        {
            return Err(AuthError::InvalidTokenResponse(
                "scope exceeds limits".to_owned(),
            ));
        }
        let expires_at = self.expires_in.map(|seconds| {
            let bounded = i64::try_from(seconds.min(31_536_000)).unwrap_or(31_536_000);
            Utc::now() + ChronoDuration::seconds(bounded)
        });
        Ok(TokenSet {
            agent_id,
            access_token: SecretValue::new(self.access_token),
            refresh_token: self.refresh_token.map(SecretValue::new),
            token_type: "Bearer".to_owned(),
            expires_at,
            scope: self.scope,
        })
    }
}

#[derive(Debug, Deserialize)]
struct ProviderTokenError {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredTokenSet {
    agent_id: AgentId,
    access_token: String,
    refresh_token: Option<String>,
    token_type: String,
    expires_at: Option<DateTime<Utc>>,
    scope: Option<String>,
}

/// Secure persistence wrapper for the provider credential set.
#[derive(Clone)]
pub struct CredentialManager {
    store: Arc<dyn CredentialStore>,
}

impl CredentialManager {
    /// Creates a manager over a platform secure store.
    #[must_use]
    pub fn new(store: Arc<dyn CredentialStore>) -> Self {
        Self { store }
    }

    /// Serializes credentials only into the configured secure store.
    pub async fn save(&self, tokens: &TokenSet) -> AuthResult<()> {
        let stored = StoredTokenSet {
            agent_id: tokens.agent_id.clone(),
            access_token: tokens.access_token.expose_secret().to_owned(),
            refresh_token: tokens
                .refresh_token
                .as_ref()
                .map(|token| token.expose_secret().to_owned()),
            token_type: tokens.token_type.clone(),
            expires_at: tokens.expires_at,
            scope: tokens.scope.clone(),
        };
        let json = serde_json::to_string(&stored)
            .map_err(|error| AuthError::CredentialEncoding(error.to_string()))?;
        self.store
            .set(CREDENTIAL_ACCOUNT, SecretValue::new(json))
            .await
            .map_err(AuthError::Platform)
    }

    /// Loads credentials from secure storage.
    pub async fn load(&self) -> AuthResult<Option<TokenSet>> {
        let Some(secret) = self
            .store
            .get(CREDENTIAL_ACCOUNT)
            .await
            .map_err(AuthError::Platform)?
        else {
            return Ok(None);
        };
        let stored: StoredTokenSet = serde_json::from_str(secret.expose_secret())
            .map_err(|error| AuthError::CredentialEncoding(error.to_string()))?;
        validate_token("stored access token", &stored.access_token)?;
        if let Some(refresh) = &stored.refresh_token {
            validate_token("stored refresh token", refresh)?;
        }
        if !stored.token_type.eq_ignore_ascii_case("bearer") {
            return Err(AuthError::CredentialEncoding(
                "stored token_type must be Bearer".to_owned(),
            ));
        }
        Ok(Some(TokenSet {
            agent_id: stored.agent_id,
            access_token: SecretValue::new(stored.access_token),
            refresh_token: stored.refresh_token.map(SecretValue::new),
            token_type: stored.token_type,
            expires_at: stored.expires_at,
            scope: stored.scope,
        }))
    }

    /// Deletes local credentials after revocation or disconnect.
    pub async fn clear(&self) -> AuthResult<()> {
        self.store
            .delete(CREDENTIAL_ACCOUNT)
            .await
            .map_err(AuthError::Platform)
    }
}

/// Observable authentication lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthenticationState {
    /// No credentials are available.
    Unauthenticated,
    /// A browser authorization flow is active.
    Authorizing,
    /// Valid credentials are available.
    Authenticated,
    /// A refresh request is in flight.
    Refreshing,
    /// The provider rejected or revoked credentials.
    Revoked,
}

/// Concurrency-safe authentication state machine.
#[derive(Debug)]
pub struct AuthStateTracker {
    state: RwLock<AuthenticationState>,
}

impl Default for AuthStateTracker {
    fn default() -> Self {
        Self {
            state: RwLock::new(AuthenticationState::Unauthenticated),
        }
    }
}

impl AuthStateTracker {
    /// Returns the current state.
    pub async fn state(&self) -> AuthenticationState {
        *self.state.read().await
    }

    /// Applies a legal state transition.
    pub async fn transition(&self, requested: AuthenticationState) -> AuthResult<()> {
        let mut state = self.state.write().await;
        let allowed = matches!(
            (*state, requested),
            (
                AuthenticationState::Unauthenticated | AuthenticationState::Revoked,
                AuthenticationState::Authorizing
            ) | (
                AuthenticationState::Authorizing,
                AuthenticationState::Authenticated | AuthenticationState::Unauthenticated
            ) | (
                AuthenticationState::Authenticated,
                AuthenticationState::Refreshing
                    | AuthenticationState::Revoked
                    | AuthenticationState::Unauthenticated
            ) | (
                AuthenticationState::Refreshing,
                AuthenticationState::Authenticated | AuthenticationState::Revoked
            )
        );
        if !allowed {
            return Err(AuthError::InvalidStateTransition {
                from: *state,
                to: requested,
            });
        }
        *state = requested;
        Ok(())
    }
}

/// Authorization and credential lifecycle failures.
#[derive(Debug, Error)]
pub enum AuthError {
    /// Product or client configuration is invalid.
    #[error("invalid authorization configuration: {0}")]
    InvalidConfiguration(String),
    /// PKCE verifier did not meet RFC 7636 bounds.
    #[error("PKCE verifier must be 43 to 128 unreserved ASCII characters")]
    InvalidPkceVerifier,
    /// State value was malformed or did not match.
    #[error("authorization state is invalid")]
    InvalidState,
    /// Callback listener expired.
    #[error("authorization callback timed out")]
    CallbackTimeout,
    /// Callback request was malformed or provider returned an error.
    #[error("authorization callback failed: {0}")]
    Callback(String),
    /// Loopback socket could not be bound or read.
    #[error("authorization callback I/O failed: {0}")]
    CallbackIo(#[from] std::io::Error),
    /// Provider HTTP exchange failed.
    #[error("token exchange transport failed: {0}")]
    Http(String),
    /// Provider rejected the exchange.
    #[error("token exchange was rejected: {0}")]
    Provider(String),
    /// Provider returned malformed credentials.
    #[error("token response is invalid: {0}")]
    InvalidTokenResponse(String),
    /// Provider response exceeded its bound.
    #[error("token response was {0} bytes; maximum is {MAX_TOKEN_RESPONSE_BYTES}")]
    TokenResponseTooLarge(usize),
    /// Refresh was requested without a refresh credential.
    #[error("no refresh credential is available")]
    MissingRefreshToken,
    /// Secure credential serialization failed.
    #[error("secure credential record is invalid: {0}")]
    CredentialEncoding(String),
    /// Platform credential or browser integration failed.
    #[error(transparent)]
    Platform(#[from] PlatformError),
    /// An illegal authentication state transition was attempted.
    #[error("cannot transition authentication from {from:?} to {to:?}")]
    InvalidStateTransition {
        /// Current state.
        from: AuthenticationState,
        /// Requested state.
        to: AuthenticationState,
    },
}

/// Result alias for authorization operations.
pub type AuthResult<T> = Result<T, AuthError>;

fn validate_http_endpoint(field: &str, url: &Url) -> AuthResult<()> {
    let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"));
    if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
        return Err(AuthError::InvalidConfiguration(format!(
            "{field} must use HTTPS except on loopback"
        )));
    }
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        return Err(AuthError::InvalidConfiguration(format!(
            "{field} must not contain credentials or a fragment"
        )));
    }
    Ok(())
}

fn validate_parameter(field: &str, value: &str, maximum: usize) -> AuthResult<()> {
    if value.trim().is_empty()
        || value.trim() != value
        || value.len() > maximum
        || value.chars().any(char::is_control)
    {
        return Err(AuthError::InvalidConfiguration(format!(
            "{field} must be 1 to {maximum} bytes without surrounding whitespace or control characters"
        )));
    }
    Ok(())
}

fn validate_token(field: &str, value: &str) -> AuthResult<()> {
    if value.is_empty() || value.len() > 16 * 1024 || value.chars().any(char::is_control) {
        return Err(AuthError::InvalidTokenResponse(format!(
            "{field} is empty or exceeds safe limits"
        )));
    }
    Ok(())
}

fn truncate(value: &str, maximum: usize) -> String {
    value.chars().take(maximum).collect()
}

#[cfg(test)]
mod tests {
    use oppa_platform::MemoryCredentialStore;

    use super::*;

    #[test]
    fn pkce_matches_rfc_7636_s256_vector() {
        let pair = PkcePair::try_from_verifier("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk")
            .expect("RFC verifier");
        assert_eq!(
            pair.challenge(),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn state_validation_is_exact() {
        let state =
            AuthorizationState::from_value("0123456789abcdef0123456789abcdef").expect("state");
        assert!(state.matches("0123456789abcdef0123456789abcdef"));
        assert!(!state.matches("0123456789abcdef0123456789abcdeg"));
        assert!(!state.matches("short"));
    }

    #[tokio::test]
    async fn credentials_only_round_trip_through_secure_store_boundary() {
        let manager = CredentialManager::new(Arc::new(MemoryCredentialStore::default()));
        let tokens = TokenSet {
            agent_id: AgentId::new("agent_test").expect("agent id"),
            access_token: SecretValue::new("access"),
            refresh_token: Some(SecretValue::new("refresh")),
            token_type: "Bearer".to_owned(),
            expires_at: Some(Utc::now()),
            scope: Some("print".to_owned()),
        };
        manager.save(&tokens).await.expect("save");
        let loaded = manager.load().await.expect("load").expect("present");
        assert_eq!(loaded.access_token.expose_secret(), "access");
        assert_eq!(
            loaded.refresh_token.expect("refresh").expose_secret(),
            "refresh"
        );
        manager.clear().await.expect("clear");
        assert!(manager.load().await.expect("load").is_none());
    }

    #[tokio::test]
    async fn authentication_state_rejects_impossible_jump() {
        let state = AuthStateTracker::default();
        assert!(matches!(
            state.transition(AuthenticationState::Authenticated).await,
            Err(AuthError::InvalidStateTransition { .. })
        ));
        state
            .transition(AuthenticationState::Authorizing)
            .await
            .expect("begin");
        state
            .transition(AuthenticationState::Authenticated)
            .await
            .expect("authorized");
    }

    #[test]
    fn token_debug_is_redacted() {
        let tokens = TokenSet {
            agent_id: AgentId::new("agent_test").expect("agent id"),
            access_token: SecretValue::new("very-secret"),
            refresh_token: Some(SecretValue::new("also-secret")),
            token_type: "Bearer".to_owned(),
            expires_at: None,
            scope: None,
        };
        let debug = format!("{tokens:?}");
        assert!(!debug.contains("very-secret"));
        assert!(!debug.contains("also-secret"));
    }

    #[test]
    fn initial_token_requires_provider_issued_agent_identity() {
        let response = TokenResponse {
            agent_id: None,
            access_token: "access".to_owned(),
            refresh_token: Some("refresh".to_owned()),
            token_type: "Bearer".to_owned(),
            expires_in: Some(300),
            scope: None,
        };
        assert!(matches!(
            response.into_token_set(None),
            Err(AuthError::InvalidTokenResponse(message))
                if message.contains("agent_id")
        ));
    }

    #[test]
    fn refresh_may_preserve_prior_agent_identity() {
        let agent_id = AgentId::new("agent_provider_1").expect("agent id");
        let response = TokenResponse {
            agent_id: None,
            access_token: "refreshed-access".to_owned(),
            refresh_token: None,
            token_type: "Bearer".to_owned(),
            expires_in: Some(300),
            scope: None,
        };
        let tokens = response
            .into_token_set(Some(&agent_id))
            .expect("preserve agent id");
        assert_eq!(tokens.agent_id, agent_id);
    }
}
