//! Minimal Android and iOS runtime bridge for the OPPA Tauri host.
//!
//! The shared Rust agent owns authentication, transport, storage, rendering,
//! and printing. This plugin only exposes capabilities that are reachable
//! exclusively through the mobile runtimes:
//!
//! - Android Keystore-backed credential encryption
//!   ([`AndroidKeystoreCredentialStore`]);
//! - the Android foreground service and its persistent notification;
//! - battery-optimization, notification, and APK-install permissions;
//! - DNS-SD service discovery (`NsdManager` and `NWBrowser`);
//! - iOS keep-awake, bounded background tasks, and local network privacy;
//! - lifecycle and connectivity events delivered as [`MobileEvent`].
//!
//! It registers no frontend commands. Every call originates in the Rust host.

#![warn(missing_docs)]

mod models;

#[cfg(mobile)]
mod mobile;

#[cfg(target_os = "android")]
pub use mobile::AndroidKeystoreCredentialStore;
#[cfg(mobile)]
pub use mobile::OppaMobile;
pub use models::{AndroidBackgroundStatus, LocalNetworkAccess, MobileEvent, ServiceEndpoint};

use tauri::{
    Runtime,
    plugin::{Builder, TauriPlugin},
};

/// Structured failures from a native plugin call.
#[derive(Debug, thiserror::Error)]
pub enum MobileError {
    /// The Android activity that dispatches native calls is not alive.
    #[error("the native runtime is not attached; open the app and try again")]
    Detached,
    /// The native call did not answer before its deadline.
    #[error("native call {0} timed out")]
    Timeout(&'static str),
    /// The native implementation rejected the call.
    #[error("{0}")]
    Native(String),
}

/// Initializes the plugin. Register it only on Android and iOS.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("oppa-mobile")
        .setup(|_app, _api| {
            #[cfg(mobile)]
            {
                use tauri::Manager as _;
                let bridge = mobile::init(_app, &_api)?;
                _app.manage(bridge);
            }
            Ok(())
        })
        .build()
}

/// Access to the [`OppaMobile`] bridge from any Tauri manager.
#[cfg(mobile)]
pub trait OppaMobileExt<R: Runtime> {
    /// Returns the process-wide native bridge.
    fn oppa_mobile(&self) -> &OppaMobile<R>;
}

#[cfg(mobile)]
impl<R: Runtime, T: tauri::Manager<R>> OppaMobileExt<R> for T {
    fn oppa_mobile(&self) -> &OppaMobile<R> {
        self.state::<OppaMobile<R>>().inner()
    }
}
