//! Printer discovery with provider isolation, normalization, and deduplication.
//!
//! Providers return observations rather than authoritative identities. The
//! [`DiscoveryManager`] retains every provider's metadata, merges likely
//! duplicates, and reports inventory changes without allowing one unavailable
//! provider to abort a discovery pass.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod manager;
mod providers;

use std::time::Duration;

use async_trait::async_trait;
use oppa_printer::DiscoveredPrinter;
use thiserror::Error;

pub use manager::{
    DiscoveryManager, DiscoverySnapshot, InventoryChange, ProviderFailure, deduplicate,
    normalize_printer,
};
pub use providers::{
    ManualNetworkPrinter, ManualNetworkProvider, SystemQueueProvider, VirtualPrinterDefinition,
    VirtualPrinterProvider, parse_lpstat,
};

/// Errors produced by one discovery provider.
#[derive(Debug, Error)]
pub enum DiscoveryError {
    /// The provider is not available on the current system.
    #[error("provider is unavailable: {0}")]
    Unavailable(String),
    /// Provider execution exceeded its deadline.
    #[error("provider timed out after {0:?}")]
    Timeout(Duration),
    /// A provider returned malformed printer data.
    #[error("provider returned invalid data: {0}")]
    InvalidData(String),
    /// The platform command or API failed.
    #[error("provider operation failed: {0}")]
    Provider(String),
}

/// Result type returned by discovery providers.
pub type DiscoveryResult<T> = Result<T, DiscoveryError>;

/// Source of physical or virtual printer observations.
#[async_trait]
pub trait DiscoveryProvider: Send + Sync {
    /// Stable provider name used in diagnostics and retained metadata.
    fn name(&self) -> &'static str;

    /// Discovers currently visible printers.
    ///
    /// Returning [`DiscoveryError::Unavailable`] is expected when an optional
    /// platform capability is absent.
    async fn discover(&self) -> DiscoveryResult<Vec<DiscoveredPrinter>>;
}
