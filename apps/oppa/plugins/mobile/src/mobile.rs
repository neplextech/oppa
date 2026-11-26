//! Native plugin registration and typed calls.
//!
//! Every fallible call returns [`MobileError`]: `Detached` when no Android
//! activity can dispatch it, `Timeout` past its deadline, or `Native` when the
//! Kotlin or Swift implementation rejects it. The variants are documented on
//! the type rather than repeated on each method.
#![allow(clippy::missing_errors_doc)]

#[cfg(target_os = "android")]
use std::collections::HashMap;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use serde::{Serialize, de::DeserializeOwned};
use serde_json::json;
use tauri::{
    AppHandle, Runtime,
    ipc::{Channel, InvokeResponseBody},
    plugin::{PluginApi, PluginHandle},
};
#[cfg(target_os = "android")]
use tokio::sync::RwLock;
use tokio::sync::broadcast;

use crate::{
    MobileError, MobileEvent, ServiceEndpoint,
    models::{BooleanValue, ServiceEndpoints},
};

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_oppa_mobile);

/// Deadline for quick native calls (preferences, Keystore, intents).
const NATIVE_CALL_TIMEOUT: Duration = Duration::from_secs(10);
/// Extra time granted to discovery beyond the requested browse window.
const DISCOVERY_GRACE: Duration = Duration::from_secs(5);
/// Event fan-out capacity; slow consumers observe `Lagged` instead of blocking.
const EVENT_CAPACITY: usize = 32;

/// Process-wide handle to the Kotlin or Swift plugin instance.
pub struct OppaMobile<R: Runtime> {
    handle: PluginHandle<R>,
    events: broadcast::Sender<MobileEvent>,
    attached: Arc<AtomicBool>,
}

pub(crate) fn init<R: Runtime>(
    _app: &AppHandle<R>,
    api: &PluginApi<R, ()>,
) -> tauri::Result<OppaMobile<R>> {
    #[cfg(target_os = "android")]
    let handle = api
        .register_android_plugin("com.neplextech.oppa.mobile", "OppaMobilePlugin")
        .map_err(|error| tauri::Error::Anyhow(error.into()))?;
    #[cfg(target_os = "ios")]
    let handle = api
        .register_ios_plugin(init_plugin_oppa_mobile)
        .map_err(|error| tauri::Error::Anyhow(error.into()))?;

    let (events, _) = broadcast::channel(EVENT_CAPACITY);
    let attached = Arc::new(AtomicBool::new(true));
    let bridge = OppaMobile {
        handle,
        events,
        attached,
    };
    bridge.register_event_channel();
    Ok(bridge)
}

impl<R: Runtime> OppaMobile<R> {
    /// Subscribes to native lifecycle, connectivity, and control events.
    pub fn subscribe(&self) -> broadcast::Receiver<MobileEvent> {
        self.events.subscribe()
    }

    /// Whether native calls can currently be dispatched.
    ///
    /// On Android, Tauri dispatches plugin calls through the most recent
    /// activity. While the agent runs in the foreground service without an
    /// activity, callers must avoid native calls and rely on cached state.
    pub fn is_attached(&self) -> bool {
        self.attached.load(Ordering::Acquire)
    }

    fn register_event_channel(&self) {
        let events = self.events.clone();
        let attached = Arc::clone(&self.attached);
        let channel = Channel::<serde_json::Value>::new(move |body| {
            let InvokeResponseBody::Json(json) = body else {
                return Ok(());
            };
            if let Ok(event) = serde_json::from_str::<MobileEvent>(&json) {
                match event {
                    MobileEvent::ActivityAvailable => attached.store(true, Ordering::Release),
                    MobileEvent::ActivityDestroyed => attached.store(false, Ordering::Release),
                    _ => {}
                }
                let _ = events.send(event);
            }
            Ok(())
        });
        let handle = self.handle.clone();
        tauri::async_runtime::spawn(async move {
            let registration = handle
                .run_mobile_plugin_async::<serde_json::Value>(
                    "registerEvents",
                    json!({ "handler": channel }),
                )
                .await;
            if let Err(error) = registration {
                eprintln!("oppa-mobile: event channel registration failed: {error}");
            }
        });
    }

    async fn call<T: DeserializeOwned>(
        &self,
        command: &'static str,
        payload: impl Serialize,
        deadline: Duration,
    ) -> Result<T, MobileError> {
        if !self.is_attached() {
            return Err(MobileError::Detached);
        }
        tokio::time::timeout(
            deadline,
            self.handle.run_mobile_plugin_async::<T>(command, payload),
        )
        .await
        .map_err(|_| MobileError::Timeout(command))?
        .map_err(|error| MobileError::Native(error.to_string()))
    }

    async fn call_unit(
        &self,
        command: &'static str,
        payload: impl Serialize,
    ) -> Result<(), MobileError> {
        self.call::<serde_json::Value>(command, payload, NATIVE_CALL_TIMEOUT)
            .await
            .map(|_| ())
    }

    /// Browses DNS-SD service types for at most `window` and resolves results.
    pub async fn browse_services(
        &self,
        service_types: &[&str],
        window: Duration,
    ) -> Result<Vec<ServiceEndpoint>, MobileError> {
        let timeout_ms = u64::try_from(window.as_millis()).unwrap_or(u64::MAX);
        self.call::<ServiceEndpoints>(
            "browseServices",
            json!({ "serviceTypes": service_types, "timeoutMs": timeout_ms }),
            window + DISCOVERY_GRACE,
        )
        .await
        .map(|endpoints| endpoints.services)
    }

    /// Opens the system share sheet for plain text.
    pub async fn share_text(&self, title: &str, text: &str) -> Result<(), MobileError> {
        self.call_unit("shareText", json!({ "title": title, "text": text }))
            .await
    }

    /// Opens a validated HTTP(S) URL in the system browser.
    pub async fn open_url(&self, url: &str) -> Result<(), MobileError> {
        self.call_unit("openUrl", json!({ "url": url })).await
    }

    async fn call_bool(&self, command: &'static str) -> Result<bool, MobileError> {
        self.call::<BooleanValue>(command, json!({}), NATIVE_CALL_TIMEOUT)
            .await
            .map(|value| value.value)
    }
}

#[cfg(target_os = "android")]
impl<R: Runtime> OppaMobile<R> {
    /// Enables or disables the persistent foreground agent service.
    ///
    /// `status_path` names the sanitized status file the service observes to
    /// update its notification without calling back into the activity.
    pub async fn set_background_service(
        &self,
        enabled: bool,
        status_path: &str,
    ) -> Result<(), MobileError> {
        self.call_unit(
            "setBackgroundService",
            json!({ "enabled": enabled, "statusPath": status_path }),
        )
        .await
    }

    /// Starts the foreground service now when it is enabled and the agent is
    /// paired. Must be called while the UI is visible.
    pub async fn ensure_background_service(&self) -> Result<(), MobileError> {
        self.call_unit("ensureBackgroundService", json!({})).await
    }

    /// Reads background-execution prerequisites.
    pub async fn background_status(&self) -> Result<crate::AndroidBackgroundStatus, MobileError> {
        self.call("backgroundStatus", json!({}), NATIVE_CALL_TIMEOUT)
            .await
    }

    /// Requests `POST_NOTIFICATIONS` on Android 13 and newer.
    pub async fn request_notification_permission(&self) -> Result<bool, MobileError> {
        // The permission dialog waits for the user, so allow a longer window.
        self.call::<BooleanValue>(
            "requestNotificationPermission",
            json!({}),
            Duration::from_secs(120),
        )
        .await
        .map(|value| value.value)
    }

    /// Opens the system prompt or settings page for battery optimization.
    pub async fn request_battery_exemption(&self) -> Result<(), MobileError> {
        self.call_unit("requestBatteryExemption", json!({})).await
    }

    /// Opens this application's system settings page.
    pub async fn open_app_settings(&self) -> Result<(), MobileError> {
        self.call_unit("openAppSettings", json!({})).await
    }

    /// Whether the user allowed this app to request package installs.
    pub async fn can_install_packages(&self) -> Result<bool, MobileError> {
        self.call_bool("canInstallPackages").await
    }

    /// Opens the "install unknown apps" settings page for this app.
    pub async fn open_install_settings(&self) -> Result<(), MobileError> {
        self.call_unit("openInstallSettings", json!({})).await
    }

    /// Hands a downloaded APK to the system installer after the native side
    /// verifies that its package name and signing certificate match the
    /// installed app. The user must still approve the installation.
    pub async fn install_apk(&self, path: &str) -> Result<(), MobileError> {
        self.call_unit("installApk", json!({ "path": path })).await
    }
}

#[cfg(target_os = "ios")]
impl<R: Runtime> OppaMobile<R> {
    /// Prevents or allows screen auto-lock while the UI is active.
    pub async fn set_keep_awake(&self, enabled: bool) -> Result<(), MobileError> {
        self.call_unit("setKeepAwake", json!({ "enabled": enabled }))
            .await
    }

    /// Requests bounded background execution time for in-flight work.
    pub async fn begin_background_task(&self) -> Result<i64, MobileError> {
        self.call::<crate::models::BackgroundTask>(
            "beginBackgroundTask",
            json!({}),
            NATIVE_CALL_TIMEOUT,
        )
        .await
        .map(|task| task.task_id)
    }

    /// Ends a bounded background task early.
    pub async fn end_background_task(&self, task_id: i64) -> Result<(), MobileError> {
        self.call_unit("endBackgroundTask", json!({ "taskId": task_id }))
            .await
    }

    /// Triggers (once) and reports local network privacy authorization.
    pub async fn local_network_access(&self) -> Result<crate::LocalNetworkAccess, MobileError> {
        self.call::<crate::models::LocalNetworkState>(
            "localNetworkAccess",
            json!({}),
            Duration::from_secs(15),
        )
        .await
        .map(|state| state.access)
    }

    /// Opens this application's page in the Settings app.
    pub async fn open_app_settings(&self) -> Result<(), MobileError> {
        self.call_unit("openAppSettings", json!({})).await
    }
}

/// Android Keystore-backed implementation of [`oppa_platform::CredentialStore`].
///
/// Kotlin encrypts each secret with a non-exportable AES-256-GCM key held by
/// the Android Keystore and stores only the ciphertext in app-private,
/// backup-excluded preferences.
///
/// Values are cached in process memory after the first successful read or
/// write. Android dispatches plugin calls through an activity, so the cache is
/// what lets the agent re-authenticate from the foreground service after the
/// activity is destroyed. The cache lives no longer than the process, values
/// zero their memory on drop, and nothing is ever written in plaintext.
#[cfg(target_os = "android")]
pub struct AndroidKeystoreCredentialStore<R: Runtime> {
    handle: PluginHandle<R>,
    attached: Arc<AtomicBool>,
    cache: RwLock<HashMap<String, Option<oppa_platform::SecretValue>>>,
}

#[cfg(target_os = "android")]
impl<R: Runtime> AndroidKeystoreCredentialStore<R> {
    /// Creates a store that shares the bridge's native plugin handle.
    pub fn new(bridge: &OppaMobile<R>) -> Self {
        Self {
            handle: bridge.handle.clone(),
            attached: Arc::clone(&bridge.attached),
            cache: RwLock::new(HashMap::new()),
        }
    }

    async fn native<T: DeserializeOwned>(
        &self,
        command: &'static str,
        payload: serde_json::Value,
    ) -> oppa_platform::PlatformResult<T> {
        if !self.attached.load(Ordering::Acquire) {
            return Err(oppa_platform::PlatformError::CredentialStore(
                MobileError::Detached.to_string(),
            ));
        }
        tokio::time::timeout(
            NATIVE_CALL_TIMEOUT,
            self.handle.run_mobile_plugin_async::<T>(command, payload),
        )
        .await
        .map_err(|_| {
            oppa_platform::PlatformError::CredentialStore(MobileError::Timeout(command).to_string())
        })?
        .map_err(|error| oppa_platform::PlatformError::CredentialStore(error.to_string()))
    }
}

#[cfg(target_os = "android")]
#[async_trait::async_trait]
impl<R: Runtime> oppa_platform::CredentialStore for AndroidKeystoreCredentialStore<R> {
    async fn set(
        &self,
        account: &str,
        secret: oppa_platform::SecretValue,
    ) -> oppa_platform::PlatformResult<()> {
        validate_account(account)?;
        self.native::<serde_json::Value>(
            "credentialSet",
            json!({ "account": account, "value": secret.expose_secret() }),
        )
        .await?;
        self.cache
            .write()
            .await
            .insert(account.to_owned(), Some(secret));
        Ok(())
    }

    async fn get(
        &self,
        account: &str,
    ) -> oppa_platform::PlatformResult<Option<oppa_platform::SecretValue>> {
        validate_account(account)?;
        if let Some(cached) = self.cache.read().await.get(account) {
            return Ok(cached.clone());
        }
        let value = self
            .native::<crate::models::CredentialValue>(
                "credentialGet",
                json!({ "account": account }),
            )
            .await?
            .value
            .map(oppa_platform::SecretValue::new);
        self.cache
            .write()
            .await
            .insert(account.to_owned(), value.clone());
        Ok(value)
    }

    async fn delete(&self, account: &str) -> oppa_platform::PlatformResult<()> {
        validate_account(account)?;
        self.native::<serde_json::Value>("credentialDelete", json!({ "account": account }))
            .await?;
        self.cache.write().await.insert(account.to_owned(), None);
        Ok(())
    }
}

#[cfg(target_os = "android")]
fn validate_account(account: &str) -> oppa_platform::PlatformResult<()> {
    if account.is_empty()
        || account.trim() != account
        || account.len() > 256
        || account.chars().any(char::is_control)
    {
        return Err(oppa_platform::PlatformError::InvalidInput(
            "credential account must be 1 to 256 bytes without surrounding whitespace".to_owned(),
        ));
    }
    Ok(())
}
