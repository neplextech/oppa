use async_trait::async_trait;

use crate::{PlatformError, PlatformResult};

/// Start-on-login integration boundary.
#[async_trait]
pub trait StartupManager: Send + Sync {
    /// Returns whether start-on-login is currently enabled.
    async fn is_enabled(&self) -> PlatformResult<bool>;

    /// Enables or disables start-on-login.
    async fn set_enabled(&self, enabled: bool) -> PlatformResult<()>;
}

/// Explicit fallback used when the desktop host has no startup integration.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnsupportedStartupManager;

#[async_trait]
impl StartupManager for UnsupportedStartupManager {
    async fn is_enabled(&self) -> PlatformResult<bool> {
        Err(unsupported())
    }

    async fn set_enabled(&self, _enabled: bool) -> PlatformResult<()> {
        Err(unsupported())
    }
}

fn unsupported() -> PlatformError {
    PlatformError::Unsupported {
        capability: "start-on-login",
        platform: std::env::consts::OS,
    }
}
