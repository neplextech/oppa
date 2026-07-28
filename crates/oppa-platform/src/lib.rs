//! Portable interfaces for operating-system capabilities used by OPPA.
//!
//! Secure credentials and application data paths have concrete cross-platform
//! implementations. Other host integrations have explicit interfaces and
//! return [`PlatformError::Unsupported`] until a platform host supplies a real
//! implementation; unsupported behavior is never silently accepted.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod credentials;
mod identity;
mod notifications;
mod paths;
mod startup;

use thiserror::Error;

pub use credentials::{
    CredentialStore, KeyringCredentialStore, MemoryCredentialStore, SecretValue,
};
pub use identity::{PlatformInfo, platform_info};
pub use notifications::{
    Notification, NotificationLevel, NotificationService, UnsupportedNotificationService,
};
pub use paths::{AppPaths, resolve_app_paths};
pub use startup::{StartupManager, UnsupportedStartupManager};

/// Result type used by portable platform interfaces.
pub type PlatformResult<T> = Result<T, PlatformError>;

/// Structured failures from an operating-system capability.
#[derive(Debug, Error)]
pub enum PlatformError {
    /// Input was rejected before invoking a platform API.
    #[error("invalid platform input: {0}")]
    InvalidInput(String),
    /// The current platform or host does not implement the capability.
    #[error("{capability} is unsupported on {platform}")]
    Unsupported {
        /// Human-readable capability name.
        capability: &'static str,
        /// Rust target operating-system identifier.
        platform: &'static str,
    },
    /// A required secure credential was absent.
    #[error("secure credential was not found")]
    CredentialNotFound,
    /// The native secure store was unavailable or rejected an operation.
    #[error("secure credential store failed: {0}")]
    CredentialStore(String),
    /// An application data path could not be resolved or created.
    #[error("application data path failed: {0}")]
    DataPath(String),
    /// The operating system rejected opening a browser URL.
    #[error("could not open system browser: {0}")]
    Browser(String),
    /// A blocking platform call could not complete.
    #[error("platform task failed: {0}")]
    Task(String),
}

/// Opens provider authorization URLs using the user's system browser.
pub trait BrowserOpener: Send + Sync {
    /// Opens an HTTP(S) URL without blocking the caller on the browser process.
    fn open(&self, url: &url::Url) -> PlatformResult<()>;
}

/// Default browser integration based on the operating-system URL handler.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemBrowser;

impl BrowserOpener for SystemBrowser {
    fn open(&self, url: &url::Url) -> PlatformResult<()> {
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(PlatformError::InvalidInput(
                "browser URL must be HTTP(S) and must not embed credentials".to_owned(),
            ));
        }
        open::that_detached(url.as_str()).map_err(|error| PlatformError::Browser(error.to_string()))
    }
}

/// Power lifecycle event relevant to connection recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerEvent {
    /// The operating system is about to suspend work.
    Sleeping,
    /// The operating system resumed execution.
    Woke,
}

/// Sleep/wake event boundary implemented by a desktop or service host.
#[async_trait::async_trait]
pub trait PowerEventSource: Send + Sync {
    /// Waits for the next operating-system power lifecycle event.
    async fn next_event(&self) -> PlatformResult<PowerEvent>;
}

/// Explicit fallback when the host has no sleep/wake integration.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnsupportedPowerEventSource;

#[async_trait::async_trait]
impl PowerEventSource for UnsupportedPowerEventSource {
    async fn next_event(&self) -> PlatformResult<PowerEvent> {
        Err(PlatformError::Unsupported {
            capability: "sleep and wake awareness",
            platform: std::env::consts::OS,
        })
    }
}

/// Outcome of trying to become the sole process instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SingleInstanceOutcome {
    /// This process acquired the application-wide guard.
    Acquired,
    /// Another process already owns the guard.
    AlreadyRunning,
}

/// Portable single-instance boundary implemented by the desktop host.
pub trait SingleInstanceManager: Send + Sync {
    /// Attempts to acquire a product-specific process guard.
    fn acquire(&self, application_id: &str) -> PlatformResult<SingleInstanceOutcome>;
}

/// Explicit fallback when the host has no single-instance integration.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnsupportedSingleInstanceManager;

impl SingleInstanceManager for UnsupportedSingleInstanceManager {
    fn acquire(&self, _application_id: &str) -> PlatformResult<SingleInstanceOutcome> {
        Err(PlatformError::Unsupported {
            capability: "single-instance guard",
            platform: std::env::consts::OS,
        })
    }
}
