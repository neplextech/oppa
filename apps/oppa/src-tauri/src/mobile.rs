//! Android and iOS host integration.
//!
//! Everything here adapts the shared [`AgentService`] to a mobile runtime:
//! sandbox directories, the platform credential store, native lifecycle and
//! connectivity events, the Android foreground-service notification, iOS
//! keep-awake and bounded background tasks, and the Android update flow. No
//! agent business logic lives here.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use oppa_platform::{AppPaths, CredentialStore};
use oppa_product::ProductConfig;
use serde::Serialize;
use tauri::{App, AppHandle, Listener as _, Manager as _, RunEvent};
use tauri_plugin_oppa_mobile::{MobileEvent, OppaMobileExt as _};
use tokio::sync::RwLock;

use crate::{
    error::{CommandError, sanitize},
    lifecycle::{AgentFacts, LifecycleAction, LifecycleEvent, LifecyclePolicy},
    mobile_settings::{self, MobileLifecycleStatus, MobileSettings},
    models::OpenPrinterConnectionState,
    network_discovery::{self, DiscoveredNetworkPrinter, RawServiceEndpoint},
    service::{AgentService, JOBS_CHANGED_EVENT, PRINTERS_CHANGED_EVENT, STATE_CHANGED_EVENT},
};

/// Window during which DNS-SD browsing collects results.
const DISCOVERY_WINDOW: Duration = Duration::from_secs(4);
/// File observed by the Android foreground service for its notification.
#[cfg(target_os = "android")]
const STATUS_FILE: &str = "agent-status.json";

/// Mobile host state shared by commands and the lifecycle task.
pub struct MobileHost {
    settings: RwLock<MobileSettings>,
    policy: Mutex<LifecyclePolicy>,
    background_task: Mutex<Option<i64>>,
    #[cfg(target_os = "android")]
    pending_update: tokio::sync::Mutex<Option<crate::updates::AndroidUpdateManifest>>,
    #[cfg(target_os = "android")]
    status_path: PathBuf,
}

/// Resolves sandboxed data, cache, and log directories from the runtime.
pub fn resolve_paths(app: &AppHandle) -> Result<AppPaths, CommandError> {
    let resolver = app.path();
    let data = resolver
        .app_data_dir()
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let cache = resolver
        .app_cache_dir()
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let logs = resolver
        .app_log_dir()
        .map_err(|error| CommandError::internal(error.to_string()))?;
    AppPaths::from_directories(data, cache, logs)
        .map_err(|error| CommandError::internal(error.to_string()))
}

/// Selects the platform's persistent secure credential store.
// Only the iOS Keychain constructor can fail.
#[cfg_attr(target_os = "android", allow(clippy::unnecessary_wraps))]
pub fn credential_store(
    app: &AppHandle,
    product: &ProductConfig,
) -> Result<Arc<dyn CredentialStore>, CommandError> {
    #[cfg(target_os = "android")]
    {
        let _ = product;
        Ok(Arc::new(
            tauri_plugin_oppa_mobile::AndroidKeystoreCredentialStore::new(app.oppa_mobile()),
        ))
    }
    #[cfg(target_os = "ios")]
    {
        let _ = app;
        Ok(Arc::new(
            oppa_platform::AppleKeychainCredentialStore::new(product.application_id.clone())
                .map_err(|error| CommandError::internal(error.to_string()))?,
        ))
    }
}

/// Installs mobile lifecycle handling for the process-owned agent.
pub fn setup(app: &App, service: &Arc<AgentService>) {
    if app.try_state::<Arc<MobileHost>>().is_some() {
        return;
    }
    let settings = tauri::async_runtime::block_on(MobileSettings::load(&service.storage));
    let host = Arc::new(MobileHost {
        settings: RwLock::new(settings),
        policy: Mutex::new(LifecyclePolicy::new(
            service.capabilities.background_execution,
        )),
        background_task: Mutex::new(None),
        #[cfg(target_os = "android")]
        pending_update: tokio::sync::Mutex::new(None),
        #[cfg(target_os = "android")]
        status_path: service.paths().cache_dir.join(STATUS_FILE),
    });
    app.manage(Arc::clone(&host));

    spawn_event_loop(app.handle().clone(), Arc::clone(service), Arc::clone(&host));
    spawn_status_observer(app.handle().clone(), Arc::clone(service), Arc::clone(&host));

    let handle = app.handle().clone();
    let service = Arc::clone(service);
    tauri::async_runtime::spawn(async move {
        apply_settings(&handle, &service, &host).await;
    });
}

/// Keeps the process-owned runtime alive when the last Android activity
/// closes. Programmatic exits (`code` is set) still terminate.
pub fn handle_run_event(_app: &AppHandle, event: &RunEvent) {
    if let RunEvent::ExitRequested {
        code: None, api, ..
    } = event
    {
        api.prevent_exit();
    }
}

async fn apply_settings(app: &AppHandle, service: &Arc<AgentService>, host: &Arc<MobileHost>) {
    let settings = *host.settings.read().await;
    let bridge = app.oppa_mobile();
    #[cfg(target_os = "android")]
    {
        write_status(service, host).await;
        let path = host.status_path.to_string_lossy().into_owned();
        if let Err(error) = bridge
            .set_background_service(settings.background_agent, &path)
            .await
        {
            service.log.warn(
                "lifecycle",
                format!("Background agent is unavailable: {error}"),
            );
        }
    }
    #[cfg(target_os = "ios")]
    {
        let paired = service.agent_id.read().await.is_some();
        let _ = bridge.set_keep_awake(settings.keep_awake && paired).await;
    }
}

async fn facts(service: &AgentService) -> AgentFacts {
    AgentFacts {
        paired: service.agent_id.read().await.is_some(),
        connected: *service.connection_state.read().await == OpenPrinterConnectionState::Connected,
        pending_jobs: service.agent.handle().snapshot().await.pending_jobs,
    }
}

fn spawn_event_loop(app: AppHandle, service: Arc<AgentService>, host: Arc<MobileHost>) {
    let mut events = app.oppa_mobile().subscribe();
    let shutdown = service.shutdown.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            let event = tokio::select! {
                () = shutdown.cancelled() => break,
                event = events.recv() => event,
            };
            let event = match event {
                Ok(event) => event,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            };
            let lifecycle_event = match event {
                MobileEvent::Foregrounded => LifecycleEvent::Foregrounded,
                MobileEvent::Backgrounded => LifecycleEvent::Backgrounded,
                MobileEvent::NetworkAvailable => LifecycleEvent::NetworkAvailable,
                MobileEvent::NetworkLost => {
                    service.log.info("network", "Network connection lost.");
                    LifecycleEvent::NetworkLost
                }
                MobileEvent::StopRequested => LifecycleEvent::StopRequested,
                MobileEvent::BackgroundTaskExpired => LifecycleEvent::BackgroundTaskExpired,
                MobileEvent::ActivityAvailable | MobileEvent::ActivityDestroyed => continue,
            };
            handle_lifecycle(&app, &service, &host, lifecycle_event).await;
        }
    });
}

async fn handle_lifecycle(
    app: &AppHandle,
    service: &Arc<AgentService>,
    host: &Arc<MobileHost>,
    event: LifecycleEvent,
) {
    let facts = facts(service).await;
    let actions = match host.policy.lock() {
        Ok(mut policy) => policy.on_event(event, facts, Instant::now()),
        Err(_) => return,
    };
    for action in actions {
        match action {
            LifecycleAction::Reconnect => {
                service.log.info(
                    "lifecycle",
                    "Reconnecting after a lifecycle or network change.",
                );
                if let Err(error) = service.reconnect().await {
                    service.log.warn("lifecycle", error.to_string());
                }
            }
            LifecycleAction::BeginBackgroundTask => begin_background_task(app, service, host).await,
            LifecycleAction::EndBackgroundTask => end_background_task(app, host).await,
            LifecycleAction::Shutdown => stop_agent(app, service).await,
        }
    }
}

#[cfg(target_os = "ios")]
async fn begin_background_task(app: &AppHandle, service: &AgentService, host: &MobileHost) {
    match app.oppa_mobile().begin_background_task().await {
        Ok(task_id) => {
            if let Ok(mut task) = host.background_task.lock() {
                *task = Some(task_id);
            }
            service.log.info(
                "lifecycle",
                "Requested limited background time to finish in-flight jobs.",
            );
        }
        Err(error) => service.log.warn("lifecycle", error.to_string()),
    }
}

#[cfg(target_os = "android")]
#[allow(clippy::unused_async)] // Signature shared with the iOS implementation.
async fn begin_background_task(_app: &AppHandle, _service: &AgentService, _host: &MobileHost) {}

#[cfg_attr(target_os = "android", allow(clippy::unused_async))]
async fn end_background_task(app: &AppHandle, host: &MobileHost) {
    let task = host
        .background_task
        .lock()
        .ok()
        .and_then(|mut task| task.take());
    #[cfg(target_os = "ios")]
    if let Some(task_id) = task {
        let _ = app.oppa_mobile().end_background_task(task_id).await;
    }
    #[cfg(target_os = "android")]
    let _ = (app, task);
}

/// Stops the agent and exits the process (Android "Stop agent").
pub async fn stop_agent(app: &AppHandle, service: &Arc<AgentService>) {
    service.log.info("lifecycle", "Stopping the agent.");
    service.shutdown().await;
    app.exit(0);
    // The Android event loop may outlive `exit`; make sure the process ends
    // so no orphaned runtime keeps a connection or processes jobs.
    tokio::time::sleep(Duration::from_millis(750)).await;
    std::process::exit(0);
}

/// Reacts to agent state changes: keeps the Android notification current,
/// starts the foreground service after pairing, applies iOS keep-awake, and
/// releases iOS background time once in-flight jobs are drained.
fn spawn_status_observer(app: AppHandle, service: Arc<AgentService>, host: Arc<MobileHost>) {
    let (notify, mut changes) = tokio::sync::mpsc::channel::<()>(1);
    for event in [
        STATE_CHANGED_EVENT,
        JOBS_CHANGED_EVENT,
        PRINTERS_CHANGED_EVENT,
    ] {
        let notify = notify.clone();
        app.listen(event, move |_| {
            let _ = notify.try_send(());
        });
    }
    let shutdown = service.shutdown.clone();
    tauri::async_runtime::spawn(async move {
        let mut was_paired = service.agent_id.read().await.is_some();
        loop {
            tokio::select! {
                () = shutdown.cancelled() => break,
                change = changes.recv() => if change.is_none() { break },
            }
            // Coalesce bursts of events into one update.
            tokio::time::sleep(Duration::from_millis(250)).await;
            while changes.try_recv().is_ok() {}

            let facts = facts(&service).await;
            #[cfg(target_os = "android")]
            write_status(&service, &host).await;
            if facts.paired != was_paired {
                was_paired = facts.paired;
                if facts.paired && app.oppa_mobile().is_attached() {
                    apply_settings(&app, &service, &host).await;
                    #[cfg(target_os = "android")]
                    if host.settings.read().await.background_agent {
                        // Pairing happens in the visible UI, so this is the
                        // moment to ask once for the status notification.
                        let _ = app.oppa_mobile().request_notification_permission().await;
                        let _ = app.oppa_mobile().ensure_background_service().await;
                    }
                }
                #[cfg(target_os = "ios")]
                if !facts.paired {
                    let _ = app.oppa_mobile().set_keep_awake(false).await;
                }
            }
            if facts.pending_jobs == 0 {
                handle_lifecycle(&app, &service, &host, LifecycleEvent::JobsDrained).await;
            }
        }
    });
}

#[cfg(target_os = "android")]
#[derive(Serialize)]
struct NotificationStatus {
    paired: bool,
    connected: bool,
    title: String,
    text: String,
}

/// Atomically writes the sanitized notification status.
#[cfg(target_os = "android")]
async fn write_status(service: &AgentService, host: &MobileHost) {
    let state = *service.connection_state.read().await;
    let paired = service.agent_id.read().await.is_some();
    let connected = state == OpenPrinterConnectionState::Connected;
    let server = service
        .connected_service
        .read()
        .await
        .as_ref()
        .map(|server| server.name.clone());
    let pending = service.agent.handle().snapshot().await.pending_jobs;
    let (available, enabled) = service.catalog.availability_counts().await;
    let phase = match state {
        OpenPrinterConnectionState::Connected => "Connected",
        OpenPrinterConnectionState::Connecting | OpenPrinterConnectionState::Authenticating => {
            "Connecting…"
        }
        OpenPrinterConnectionState::AuthenticationFailed
        | OpenPrinterConnectionState::CredentialRevoked => "Pairing needs attention",
        _ => "Reconnecting…",
    };
    let title = match server {
        Some(server) if connected => format!("{} · {phase}", sanitize(&server)),
        _ => format!("{} · {phase}", service.product.product_name),
    };
    let text = format!("{pending} pending · {available}/{enabled} printers available");
    let status = NotificationStatus {
        paired,
        connected,
        title,
        text,
    };
    let Ok(bytes) = serde_json::to_vec(&status) else {
        return;
    };
    let temporary = host.status_path.with_extension("json.tmp");
    if tokio::fs::write(&temporary, bytes).await.is_ok() {
        let _ = tokio::fs::rename(&temporary, &host.status_path).await;
    }
}

/// Builds the lifecycle status shown by the mobile UI.
pub async fn lifecycle_status(
    app: &AppHandle,
    service: &AgentService,
) -> Result<MobileLifecycleStatus, CommandError> {
    let host = mobile_host(app)?;
    let settings = *host.settings.read().await;
    let capabilities = service.capabilities;
    let mut status = MobileLifecycleStatus {
        platform: capabilities.kind,
        background_execution: capabilities.background_execution,
        settings,
        background_service_running: false,
        notifications_granted: None,
        ignoring_battery_optimizations: None,
        local_network_access: None,
        battery_guidance: None,
        limitations: mobile_settings::limitations(capabilities),
    };
    #[cfg(target_os = "android")]
    {
        let native = app
            .oppa_mobile()
            .background_status()
            .await
            .map_err(native_error)?;
        status.background_service_running = native.running;
        status.notifications_granted = Some(native.notifications_granted);
        status.ignoring_battery_optimizations = Some(native.ignoring_battery_optimizations);
        status.battery_guidance = mobile_settings::battery_guidance(&native.manufacturer);
    }
    Ok(status)
}

fn mobile_host(app: &AppHandle) -> Result<Arc<MobileHost>, CommandError> {
    app.try_state::<Arc<MobileHost>>()
        .map(|state| Arc::clone(state.inner()))
        .ok_or_else(|| CommandError::internal("Mobile lifecycle is not initialized."))
}

#[allow(clippy::needless_pass_by_value)] // Used as a `map_err` adapter.
fn native_error(error: tauri_plugin_oppa_mobile::MobileError) -> CommandError {
    CommandError::new("native_unavailable", sanitize(&error.to_string()))
}

/// Android: enables or disables the persistent background agent.
pub async fn set_background_agent(
    app: &AppHandle,
    service: &Arc<AgentService>,
    enabled: bool,
) -> Result<(), CommandError> {
    #[cfg(target_os = "android")]
    {
        let host = mobile_host(app)?;
        let mut settings = *host.settings.read().await;
        settings.background_agent = enabled;
        settings.save(&service.storage).await?;
        *host.settings.write().await = settings;
        if enabled {
            // Ask once so the persistent notification is visible.
            let _ = app.oppa_mobile().request_notification_permission().await;
        }
        write_status(service, &host).await;
        app.oppa_mobile()
            .set_background_service(enabled, &host.status_path.to_string_lossy())
            .await
            .map_err(native_error)?;
        service.log.info(
            "lifecycle",
            if enabled {
                "Background agent enabled."
            } else {
                "Background agent disabled; printing continues only while OPPA is open."
            },
        );
        Ok(())
    }
    #[cfg(target_os = "ios")]
    {
        let _ = (app, service, enabled);
        Err(unsupported("A persistent background agent"))
    }
}

/// iOS: keeps the screen awake while the agent is paired and active.
#[cfg_attr(target_os = "android", allow(clippy::unused_async))]
pub async fn set_keep_awake(
    app: &AppHandle,
    service: &Arc<AgentService>,
    enabled: bool,
) -> Result<(), CommandError> {
    #[cfg(target_os = "ios")]
    {
        let host = mobile_host(app)?;
        let mut settings = *host.settings.read().await;
        settings.keep_awake = enabled;
        settings.save(&service.storage).await?;
        *host.settings.write().await = settings;
        apply_settings(app, service, &host).await;
        Ok(())
    }
    #[cfg(target_os = "android")]
    {
        let _ = (app, service, enabled);
        Err(unsupported("Keep-awake mode"))
    }
}

pub fn unsupported(capability: &str) -> CommandError {
    CommandError::new(
        "feature_unavailable",
        format!("{capability} is not available on this platform."),
    )
}

/// Requests the notification permission (Android 13+).
pub async fn request_notification_permission(app: &AppHandle) -> Result<bool, CommandError> {
    #[cfg(target_os = "android")]
    {
        app.oppa_mobile()
            .request_notification_permission()
            .await
            .map_err(native_error)
    }
    #[cfg(target_os = "ios")]
    {
        let _ = app;
        Err(unsupported("Notification permission"))
    }
}

/// Opens the battery-optimization exemption prompt (Android).
pub async fn request_battery_exemption(app: &AppHandle) -> Result<(), CommandError> {
    #[cfg(target_os = "android")]
    {
        app.oppa_mobile()
            .request_battery_exemption()
            .await
            .map_err(native_error)
    }
    #[cfg(target_os = "ios")]
    {
        let _ = app;
        Err(unsupported("Battery optimization settings"))
    }
}

/// Android: opens the vendor battery guide selected for this manufacturer.
///
/// The URL comes from [`mobile_settings::battery_guidance`], never from the
/// frontend, so this cannot be used to open arbitrary pages.
pub async fn open_battery_guide(app: &AppHandle) -> Result<(), CommandError> {
    #[cfg(target_os = "android")]
    {
        let native = app
            .oppa_mobile()
            .background_status()
            .await
            .map_err(native_error)?;
        let guidance = mobile_settings::battery_guidance(&native.manufacturer)
            .ok_or_else(|| unsupported("A device-specific battery guide"))?;
        let url = url::Url::parse(&guidance.reference_url)
            .map_err(|error| CommandError::internal(error.to_string()))?;
        open_url(app, &url).await
    }
    #[cfg(target_os = "ios")]
    {
        let _ = app;
        Err(unsupported("Battery guides"))
    }
}

/// Opens this app's page in system settings.
pub async fn open_app_settings(app: &AppHandle) -> Result<(), CommandError> {
    app.oppa_mobile()
        .open_app_settings()
        .await
        .map_err(native_error)
}

/// iOS: triggers and reports the local network privacy decision.
#[cfg_attr(target_os = "android", allow(clippy::unused_async))]
pub async fn request_local_network_access(app: &AppHandle) -> Result<String, CommandError> {
    #[cfg(target_os = "ios")]
    {
        let access = app
            .oppa_mobile()
            .local_network_access()
            .await
            .map_err(native_error)?;
        Ok(match access {
            tauri_plugin_oppa_mobile::LocalNetworkAccess::Granted => "granted",
            tauri_plugin_oppa_mobile::LocalNetworkAccess::Denied => "denied",
            tauri_plugin_oppa_mobile::LocalNetworkAccess::Undetermined => "undetermined",
        }
        .to_owned())
    }
    #[cfg(target_os = "android")]
    {
        // Android grants local network access with INTERNET.
        let _ = app;
        Ok("granted".to_owned())
    }
}

/// Browses DNS-SD printer services and returns add-printer suggestions.
pub async fn discover_network_printers(
    app: &AppHandle,
    service: &AgentService,
) -> Result<Vec<DiscoveredNetworkPrinter>, CommandError> {
    let endpoints = app
        .oppa_mobile()
        .browse_services(network_discovery::PRINTER_SERVICE_TYPES, DISCOVERY_WINDOW)
        .await
        .map_err(native_error)?;
    let configured = service.catalog.network_endpoints().await;
    let printers = network_discovery::normalize(
        endpoints
            .into_iter()
            .map(|endpoint| RawServiceEndpoint {
                name: endpoint.name,
                service_type: endpoint.service_type,
                host: endpoint.host,
                port: endpoint.port,
            })
            .collect(),
        &configured,
    );
    service.log.info(
        "discovery",
        format!(
            "Network discovery found {} printer service(s).",
            printers.len()
        ),
    );
    Ok(printers)
}

/// Shares the sanitized diagnostics bundle through the system share sheet.
pub async fn share_diagnostics(
    app: &AppHandle,
    service: &AgentService,
) -> Result<(), CommandError> {
    let bundle = service.diagnostics_bundle().await?;
    let title = format!("{} diagnostics", service.product.product_name);
    app.oppa_mobile()
        .share_text(&title, &bundle)
        .await
        .map_err(native_error)
}

/// Opens a validated URL in the system browser.
pub async fn open_url(app: &AppHandle, url: &url::Url) -> Result<(), CommandError> {
    app.oppa_mobile()
        .open_url(url.as_str())
        .await
        .map_err(|error| CommandError::new("browser_failed", sanitize(&error.to_string())))
}

/// Android: checks the direct-download release channel.
pub async fn check_update(
    app: &AppHandle,
) -> Result<Option<crate::updates::MobileUpdate>, CommandError> {
    #[cfg(target_os = "android")]
    {
        let host = mobile_host(app)?;
        let version = app.package_info().version.to_string();
        let checked = crate::updates::android::check(&version).await?;
        let offer = checked.as_ref().map(|(_, offer)| offer.clone());
        *host.pending_update.lock().await = checked.map(|(manifest, _)| manifest);
        Ok(offer)
    }
    #[cfg(target_os = "ios")]
    {
        let _ = app;
        Err(unsupported("In-app updates"))
    }
}

/// Outcome of an install request.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallOutcome {
    /// The system installer is showing the update for approval.
    InstallerOpened,
    /// The user must allow installs from OPPA first; settings were opened.
    PermissionRequired,
}

/// Android: downloads, verifies, and opens the installer for the offered update.
pub async fn install_update(
    app: &AppHandle,
    service: &AgentService,
) -> Result<InstallOutcome, CommandError> {
    #[cfg(target_os = "android")]
    {
        let host = mobile_host(app)?;
        let bridge = app.oppa_mobile();
        if !bridge.can_install_packages().await.map_err(native_error)? {
            bridge.open_install_settings().await.map_err(native_error)?;
            return Ok(InstallOutcome::PermissionRequired);
        }
        let manifest = host
            .pending_update
            .lock()
            .await
            .clone()
            .ok_or_else(|| CommandError::new("no_update", "Check for updates first."))?;
        service.log.info(
            "update",
            format!("Downloading OPPA {} for Android.", manifest.version),
        );
        let path = crate::updates::android::download(&manifest, &service.paths().cache_dir).await?;
        bridge
            .install_apk(&path.to_string_lossy())
            .await
            .map_err(|error| {
                CommandError::new("update_install_failed", sanitize(&error.to_string()))
            })?;
        Ok(InstallOutcome::InstallerOpened)
    }
    #[cfg(target_os = "ios")]
    {
        let _ = (app, service);
        Err(unsupported("In-app updates"))
    }
}
