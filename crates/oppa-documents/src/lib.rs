//! Authenticated, bounded, and integrity-checked retrieval of job PDFs.
//!
//! A print job carries only a URL, a SHA-256 digest, and an exact byte length.
//! This crate fetches the bytes with a request signed by the agent's Ed25519
//! credential, so no bearer secret is ever stored in a job or in `SQLite`. The
//! response is accepted only when it has exactly the declared length and the
//! declared digest. Redirects are refused, and all reads have deadlines.
//!
//! # Request signature
//!
//! The signed payload is these LF-separated lines, in order:
//!
//! ```text
//! OPENPRINTER-DOCUMENT-V1
//! GET
//! <path and query exactly as sent>
//! <agent id>
//! <job id>
//! <timestamp in Unix milliseconds>
//! <lowercase hexadecimal SHA-256>
//! ```
//!
//! The request carries `X-OpenPrinter-Agent-Id`, `X-OpenPrinter-Job-Id`,
//! `X-OpenPrinter-Timestamp`, and `X-OpenPrinter-Signature` (unpadded
//! base64url). Servers must verify the signature against the agent's
//! registered public key and reject timestamps outside a small skew window.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use oppa_auth::AgentKeyManager;
use oppa_protocol::{PdfSource, Validate};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::time::timeout;

/// Domain-separation tag at the start of every signed document request.
pub const SIGNATURE_CONTEXT: &str = "OPENPRINTER-DOCUMENT-V1";
/// Default deadline covering connection, headers, and body.
pub const DEFAULT_FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// Retrieves the verified bytes of a PDF referenced by a print job.
#[async_trait]
pub trait DocumentFetcher: Send + Sync {
    /// Fetches and verifies the exact PDF bytes described by `source`.
    ///
    /// # Errors
    ///
    /// Returns a [`DocumentFetchError`] when the source is invalid, the
    /// request is not authorized, the transport fails, the size or digest does
    /// not match, or the deadline passes.
    async fn fetch_pdf(
        &self,
        job_id: &str,
        source: &PdfSource,
    ) -> Result<Vec<u8>, DocumentFetchError>;
}

/// Failure while retrieving or verifying a job document.
#[derive(Debug, Error)]
pub enum DocumentFetchError {
    /// The job's document reference is malformed or unsupported.
    #[error("the document reference is invalid: {0}")]
    InvalidSource(String),
    /// The local agent credential could not sign the request.
    #[error("the document request could not be authorized: {0}")]
    Authorization(String),
    /// The server refused the authenticated request.
    #[error("the server refused the document request with HTTP {status}")]
    Refused {
        /// HTTP status returned by the server.
        status: u16,
    },
    /// The document no longer exists on the server.
    #[error("the server no longer has this document (HTTP {status})")]
    NotFound {
        /// HTTP status returned by the server.
        status: u16,
    },
    /// The server returned a server-side failure.
    #[error("the server failed to provide the document (HTTP {status})")]
    ServerError {
        /// HTTP status returned by the server.
        status: u16,
    },
    /// The network connection failed.
    #[error("the document could not be downloaded: {0}")]
    Transport(String),
    /// The download exceeded its deadline.
    #[error("the document download did not finish within {0:?}")]
    Timeout(Duration),
    /// The body length differed from the declared length.
    #[error("the document is {actual} bytes; the job declares {expected} bytes")]
    LengthMismatch {
        /// Declared byte length.
        expected: u64,
        /// Observed or advertised byte length.
        actual: u64,
    },
    /// The body digest differed from the declared digest.
    #[error("the document failed its integrity check")]
    IntegrityMismatch,
}

impl DocumentFetchError {
    /// Returns the stable machine-readable failure code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidSource(_) => "document.invalid_source",
            Self::Authorization(_) => "document.authorization_failed",
            Self::Refused { .. } => "document.refused",
            Self::NotFound { .. } => "document.not_found",
            Self::ServerError { .. } | Self::Transport(_) | Self::Timeout(_) => {
                "document.fetch_failed"
            }
            Self::LengthMismatch { .. } | Self::IntegrityMismatch => "document.integrity_failed",
        }
    }

    /// Returns whether repeating the same job may succeed later.
    #[must_use]
    pub const fn retryable(&self) -> bool {
        matches!(
            self,
            Self::ServerError { .. } | Self::Transport(_) | Self::Timeout(_)
        )
    }
}

/// HTTPS document fetcher that signs each request with the agent credential.
#[derive(Clone)]
pub struct HttpDocumentFetcher {
    client: reqwest::Client,
    keys: AgentKeyManager,
    credential_ref: String,
    agent_id: String,
    timeout: Duration,
}

impl HttpDocumentFetcher {
    /// Creates a fetcher for one installed agent identity.
    ///
    /// # Errors
    ///
    /// Returns [`DocumentFetchError::Transport`] when the HTTP client cannot be
    /// constructed with the required redirect and timeout policy.
    pub fn new(
        keys: AgentKeyManager,
        credential_ref: impl Into<String>,
        agent_id: impl Into<String>,
        timeout: Duration,
    ) -> Result<Self, DocumentFetchError> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(timeout)
            .build()
            .map_err(|error| DocumentFetchError::Transport(error.to_string()))?;
        Ok(Self {
            client,
            keys,
            credential_ref: credential_ref.into(),
            agent_id: agent_id.into(),
            timeout,
        })
    }
}

#[async_trait]
impl DocumentFetcher for HttpDocumentFetcher {
    async fn fetch_pdf(
        &self,
        job_id: &str,
        source: &PdfSource,
    ) -> Result<Vec<u8>, DocumentFetchError> {
        source
            .validate()
            .map_err(|error| DocumentFetchError::InvalidSource(error.to_string()))?;
        let PdfSource::Url {
            url,
            sha256,
            byte_length,
        } = source;
        let parsed = url::Url::parse(url)
            .map_err(|_| DocumentFetchError::InvalidSource("the URL is not absolute".to_owned()))?;
        let path_and_query = match parsed.query() {
            Some(query) => format!("{}?{query}", parsed.path()),
            None => parsed.path().to_owned(),
        };
        let timestamp_ms = unix_milliseconds();
        let payload = canonical_request(
            &path_and_query,
            &self.agent_id,
            job_id,
            timestamp_ms,
            sha256,
        );
        let signature = self
            .keys
            .sign(&self.credential_ref, payload.as_bytes())
            .await
            .map_err(|error| DocumentFetchError::Authorization(error.to_string()))?;

        let response = timeout(
            self.timeout,
            self.client
                .get(parsed)
                .header("X-OpenPrinter-Agent-Id", &self.agent_id)
                .header("X-OpenPrinter-Job-Id", job_id)
                .header("X-OpenPrinter-Timestamp", timestamp_ms.to_string())
                .header("X-OpenPrinter-Signature", URL_SAFE_NO_PAD.encode(signature))
                .send(),
        )
        .await
        .map_err(|_| DocumentFetchError::Timeout(self.timeout))?
        .map_err(|error| DocumentFetchError::Transport(sanitize_transport(&error)))?;

        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND || status == reqwest::StatusCode::GONE {
            return Err(DocumentFetchError::NotFound {
                status: status.as_u16(),
            });
        }
        if status.is_server_error() {
            return Err(DocumentFetchError::ServerError {
                status: status.as_u16(),
            });
        }
        if !status.is_success() {
            return Err(DocumentFetchError::Refused {
                status: status.as_u16(),
            });
        }
        if let Some(advertised) = response.content_length()
            && advertised != *byte_length
        {
            return Err(DocumentFetchError::LengthMismatch {
                expected: *byte_length,
                actual: advertised,
            });
        }

        let body = timeout(self.timeout, read_bounded(response, *byte_length))
            .await
            .map_err(|_| DocumentFetchError::Timeout(self.timeout))??;
        verify_digest(&body, sha256)?;
        Ok(body)
    }
}

async fn read_bounded(
    mut response: reqwest::Response,
    expected: u64,
) -> Result<Vec<u8>, DocumentFetchError> {
    let capacity = usize::try_from(expected).unwrap_or(0);
    let mut body = Vec::with_capacity(capacity);
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| DocumentFetchError::Transport(sanitize_transport(&error)))?
    {
        let received = body.len() as u64 + chunk.len() as u64;
        if received > expected {
            return Err(DocumentFetchError::LengthMismatch {
                expected,
                actual: received,
            });
        }
        body.extend_from_slice(&chunk);
    }
    if body.len() as u64 != expected {
        return Err(DocumentFetchError::LengthMismatch {
            expected,
            actual: body.len() as u64,
        });
    }
    Ok(body)
}

/// Checks the exact body against a lowercase hexadecimal SHA-256 digest.
///
/// # Errors
///
/// Returns [`DocumentFetchError::IntegrityMismatch`] when the digests differ.
pub fn verify_digest(body: &[u8], expected_hex: &str) -> Result<(), DocumentFetchError> {
    if sha256_hex(body) == expected_hex {
        Ok(())
    } else {
        Err(DocumentFetchError::IntegrityMismatch)
    }
}

/// Returns the lowercase hexadecimal SHA-256 digest of `body`.
#[must_use]
pub fn sha256_hex(body: &[u8]) -> String {
    Sha256::digest(body)
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            use std::fmt::Write as _;
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

/// Builds the exact string signed for one document request.
#[must_use]
pub fn canonical_request(
    path_and_query: &str,
    agent_id: &str,
    job_id: &str,
    timestamp_ms: u64,
    sha256: &str,
) -> String {
    format!(
        "{SIGNATURE_CONTEXT}\nGET\n{path_and_query}\n{agent_id}\n{job_id}\n{timestamp_ms}\n{sha256}"
    )
}

fn unix_milliseconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

fn sanitize_transport(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "the connection timed out".to_owned()
    } else if error.is_connect() {
        "the server could not be reached".to_owned()
    } else {
        "the transfer ended unexpectedly".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_request_binds_every_identity_and_the_digest() {
        let payload = canonical_request("/d/job_1?v=1", "agent_1", "job_1", 1_700, "ab");
        assert_eq!(
            payload,
            "OPENPRINTER-DOCUMENT-V1\nGET\n/d/job_1?v=1\nagent_1\njob_1\n1700\nab"
        );
    }

    #[test]
    fn digest_verification_accepts_exact_bytes_only() {
        let body = b"%PDF-1.4 fixture";
        let hex = sha256_hex(body);
        verify_digest(body, &hex).expect("matching digest");
        assert!(matches!(
            verify_digest(b"%PDF-1.4 changed", &hex),
            Err(DocumentFetchError::IntegrityMismatch)
        ));
    }

    #[test]
    fn integrity_and_length_failures_are_not_retried() {
        assert!(!DocumentFetchError::IntegrityMismatch.retryable());
        assert!(!DocumentFetchError::NotFound { status: 404 }.retryable());
        assert!(DocumentFetchError::Transport("reset".to_owned()).retryable());
        assert_eq!(
            DocumentFetchError::LengthMismatch {
                expected: 1,
                actual: 2
            }
            .code(),
            "document.integrity_failed"
        );
    }
}
