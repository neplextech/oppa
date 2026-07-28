use async_trait::async_trait;

use crate::{PlatformError, PlatformResult};

/// Severity of a user-facing operating-system notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationLevel {
    /// Informational status.
    Information,
    /// Action may soon be required.
    Warning,
    /// An operation failed and likely needs attention.
    Error,
}

/// Bounded user-facing notification content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    /// Short title.
    pub title: String,
    /// Plain-text body.
    pub body: String,
    /// Severity hint.
    pub level: NotificationLevel,
}

/// Operating-system notification boundary.
#[async_trait]
pub trait NotificationService: Send + Sync {
    /// Shows a non-sensitive notification.
    async fn show(&self, notification: &Notification) -> PlatformResult<()>;
}

/// Explicit fallback used when the host has no notification integration.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnsupportedNotificationService;

#[async_trait]
impl NotificationService for UnsupportedNotificationService {
    async fn show(&self, _notification: &Notification) -> PlatformResult<()> {
        Err(PlatformError::Unsupported {
            capability: "notifications",
            platform: std::env::consts::OS,
        })
    }
}
