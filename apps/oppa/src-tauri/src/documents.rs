//! Host adapter that gives the agent authenticated access to job PDFs.
//!
//! The agent's job processor depends only on the `DocumentFetcher` trait. This
//! adapter reads the current paired identity at fetch time, so pairing,
//! re-pairing, and forgetting a server take effect without rebuilding the agent.

use std::sync::Arc;

use async_trait::async_trait;
use oppa_auth::AgentKeyManager;
use oppa_documents::{
    DEFAULT_FETCH_TIMEOUT, DocumentFetchError, DocumentFetcher, HttpDocumentFetcher,
};
use oppa_protocol::PdfSource;
use tokio::sync::RwLock;

/// Identity used to sign document requests for the currently paired server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DocumentIdentity {
    /// Agent identity registered with the server.
    pub(crate) agent_id: String,
    /// Opaque reference to the agent's private key in secure storage.
    pub(crate) credential_ref: String,
}

/// Shared, replaceable identity cell owned by the desktop service.
pub(crate) type SharedDocumentIdentity = Arc<RwLock<Option<DocumentIdentity>>>;

/// Fetches job PDFs with the currently paired agent credential.
pub(crate) struct PairedDocumentFetcher {
    keys: AgentKeyManager,
    identity: SharedDocumentIdentity,
}

impl PairedDocumentFetcher {
    /// Creates a fetcher that reads `identity` each time a document is requested.
    pub(crate) fn new(keys: AgentKeyManager, identity: SharedDocumentIdentity) -> Self {
        Self { keys, identity }
    }
}

#[async_trait]
impl DocumentFetcher for PairedDocumentFetcher {
    async fn fetch_pdf(
        &self,
        job_id: &str,
        source: &PdfSource,
    ) -> Result<Vec<u8>, DocumentFetchError> {
        let identity = self.identity.read().await.clone().ok_or_else(|| {
            DocumentFetchError::Authorization("this agent is not paired with a server".to_owned())
        })?;
        HttpDocumentFetcher::new(
            self.keys.clone(),
            identity.credential_ref,
            identity.agent_id,
            DEFAULT_FETCH_TIMEOUT,
        )?
        .fetch_pdf(job_id, source)
        .await
    }
}
