use std::sync::Arc;

use tauri::{AppHandle, State};

use crate::{
    error::CommandError,
    mobile_settings::MobileLifecycleStatus,
    models::{
        AgentStatus, ConfigurePrinterChanges, DeepLinkPayload, Diagnostics,
        DiscoveredServiceSummary, JobSummary, ManualPrinterInput, PendingDeepLink, PrinterSummary,
        ProductLink, RecentServer, VirtualPrinterInput, VirtualPrinterMode,
    },
    network_discovery::DiscoveredNetworkPrinter,
    server_configuration::{OpenPrinterServerConfiguration, OpenPrinterServerConfigurationInput},
    service::AgentService,
    updates::MobileUpdate,
};

/// Whether start-on-login is enabled; always `false` where unsupported.
fn start_on_login_enabled(app: &AppHandle) -> bool {
    #[cfg(desktop)]
    {
        use tauri_plugin_autostart::ManagerExt as _;
        app.autolaunch().is_enabled().unwrap_or(false)
    }
    #[cfg(mobile)]
    {
        let _ = app;
        false
    }
}

#[cfg(desktop)]
fn desktop_only<T>(capability: &str) -> Result<T, CommandError> {
    Err(CommandError::new(
        "feature_unavailable",
        format!("{capability} is only available on Android and iOS."),
    ))
}

#[tauri::command]
pub async fn get_agent_status(
    app: AppHandle,
    service: State<'_, Arc<AgentService>>,
) -> Result<AgentStatus, CommandError> {
    let version = app.package_info().version.to_string();
    Ok(service.status(start_on_login_enabled(&app), version).await)
}

#[tauri::command]
pub async fn discover_server(
    service: State<'_, Arc<AgentService>>,
) -> Result<DiscoveredServiceSummary, CommandError> {
    service.discover_server().await
}

#[tauri::command]
pub async fn pair_server(
    service: State<'_, Arc<AgentService>>,
    code: String,
    agent_name: String,
) -> Result<DiscoveredServiceSummary, CommandError> {
    service.pair_server(code, agent_name).await
}

#[tauri::command]
pub async fn forget_server(service: State<'_, Arc<AgentService>>) -> Result<(), CommandError> {
    service.forget_server().await
}

#[tauri::command]
pub async fn list_printers(
    service: State<'_, Arc<AgentService>>,
) -> Result<Vec<PrinterSummary>, CommandError> {
    service.list_printers().await
}

#[tauri::command]
pub async fn refresh_printers(
    service: State<'_, Arc<AgentService>>,
) -> Result<Vec<PrinterSummary>, CommandError> {
    service.refresh_printers().await
}

#[tauri::command]
pub async fn configure_printer(
    service: State<'_, Arc<AgentService>>,
    printer_id: String,
    changes: ConfigurePrinterChanges,
) -> Result<PrinterSummary, CommandError> {
    service.configure_printer(&printer_id, changes).await
}

#[tauri::command]
pub async fn add_manual_printer(
    service: State<'_, Arc<AgentService>>,
    input: ManualPrinterInput,
) -> Result<PrinterSummary, CommandError> {
    service.add_manual_printer(input).await
}

#[tauri::command]
pub async fn remove_printer(
    service: State<'_, Arc<AgentService>>,
    printer_id: String,
) -> Result<(), CommandError> {
    service.remove_printer(&printer_id).await
}

#[tauri::command]
pub async fn create_virtual_printer(
    service: State<'_, Arc<AgentService>>,
    input: VirtualPrinterInput,
) -> Result<PrinterSummary, CommandError> {
    service.create_virtual_printer(input).await
}

#[tauri::command]
pub async fn update_virtual_printer(
    service: State<'_, Arc<AgentService>>,
    printer_id: String,
    mode: VirtualPrinterMode,
    delay_ms: u64,
) -> Result<PrinterSummary, CommandError> {
    service
        .update_virtual_printer(&printer_id, mode, delay_ms)
        .await
}

#[tauri::command]
pub async fn clear_virtual_history(
    service: State<'_, Arc<AgentService>>,
    printer_id: String,
) -> Result<(), CommandError> {
    service.clear_virtual_history(&printer_id).await
}

#[tauri::command]
pub async fn set_virtual_printer_sound(
    service: State<'_, Arc<AgentService>>,
    printer_id: String,
    enabled: bool,
) -> Result<(), CommandError> {
    service
        .set_virtual_printer_sound(&printer_id, enabled)
        .await
}

#[tauri::command]
pub async fn send_test_print(
    service: State<'_, Arc<AgentService>>,
    printer_id: String,
) -> Result<JobSummary, CommandError> {
    service.send_test_print(&printer_id).await
}

#[tauri::command]
pub async fn send_test_pdf(
    service: State<'_, Arc<AgentService>>,
    printer_id: String,
    pdf_base64: String,
    settings: Option<oppa_protocol::PrintSettings>,
) -> Result<JobSummary, CommandError> {
    service
        .send_test_pdf(&printer_id, &pdf_base64, settings.unwrap_or_default())
        .await
}

#[tauri::command]
pub async fn list_recent_jobs(
    service: State<'_, Arc<AgentService>>,
) -> Result<Vec<JobSummary>, CommandError> {
    Ok(service.list_recent_jobs().await)
}

#[tauri::command]
pub async fn clear_jobs(service: State<'_, Arc<AgentService>>) -> Result<(), CommandError> {
    service.clear_jobs().await
}

#[tauri::command]
pub async fn clear_logs(service: State<'_, Arc<AgentService>>) -> Result<(), CommandError> {
    service.clear_logs();
    Ok(())
}

#[tauri::command]
pub async fn get_diagnostics(
    service: State<'_, Arc<AgentService>>,
) -> Result<Diagnostics, CommandError> {
    Ok(service.diagnostics().await)
}

#[tauri::command]
pub async fn export_diagnostics(
    service: State<'_, Arc<AgentService>>,
) -> Result<String, CommandError> {
    service.export_diagnostics().await
}

#[tauri::command]
pub async fn get_diagnostics_bundle(
    service: State<'_, Arc<AgentService>>,
) -> Result<String, CommandError> {
    service.diagnostics_bundle().await
}

#[tauri::command]
#[cfg(mobile)]
pub async fn set_start_on_login(
    _app: AppHandle,
    _service: State<'_, Arc<AgentService>>,
    _enabled: bool,
) -> Result<bool, CommandError> {
    Err(crate::mobile::unsupported("Start on login"))
}

#[tauri::command]
#[cfg(desktop)]
pub async fn set_start_on_login(
    app: AppHandle,
    service: State<'_, Arc<AgentService>>,
    enabled: bool,
) -> Result<bool, CommandError> {
    use tauri_plugin_autostart::ManagerExt as _;
    let manager = app.autolaunch();
    if enabled {
        manager
            .enable()
            .map_err(|error| CommandError::new("startup_failed", error.to_string()))?;
    } else {
        manager
            .disable()
            .map_err(|error| CommandError::new("startup_failed", error.to_string()))?;
    }
    let actual = manager
        .is_enabled()
        .map_err(|error| CommandError::new("startup_failed", error.to_string()))?;
    service.log.info(
        "startup",
        if actual {
            "Start on login enabled."
        } else {
            "Start on login disabled."
        },
    );
    service.emit(crate::service::STATE_CHANGED_EVENT);
    Ok(actual)
}

#[tauri::command]
pub async fn reconnect(
    app: AppHandle,
    service: State<'_, Arc<AgentService>>,
) -> Result<AgentStatus, CommandError> {
    service.reconnect().await?;
    tokio::task::yield_now().await;
    let version = app.package_info().version.to_string();
    Ok(service.status(start_on_login_enabled(&app), version).await)
}

/// Stops the agent and quits instead of hiding the window to the tray (desktop)
/// or leaving the foreground service running (Android). iOS forbids
/// programmatic termination, so it reports the action as unavailable.
#[tauri::command]
pub async fn quit_application(
    app: AppHandle,
    service: State<'_, Arc<AgentService>>,
) -> Result<(), CommandError> {
    if !service.capabilities.quit {
        return Err(CommandError::new(
            "feature_unavailable",
            "This platform does not allow apps to quit themselves.",
        ));
    }
    #[cfg(desktop)]
    crate::desktop::quit_application(&app);
    #[cfg(mobile)]
    {
        let service = Arc::clone(service.inner());
        tauri::async_runtime::spawn(async move {
            crate::mobile::stop_agent(&app, &service).await;
        });
    }
    Ok(())
}

/// Stops the agent ahead of a signed desktop update so the installer and
/// relaunch never cut an active gateway connection mid-session.
///
/// Resolves after the gateway socket is closed (or the bounded shutdown
/// deadline passes). The agent stays stopped; the frontend installs the
/// update and relaunches the process, which recovers durable jobs on start.
/// Mobile updates go through the system installer and do not use this.
#[tauri::command]
pub async fn prepare_for_update(
    app: AppHandle,
    service: State<'_, Arc<AgentService>>,
) -> Result<(), CommandError> {
    #[cfg(desktop)]
    {
        crate::desktop::prepare_for_relaunch(&app, service.inner()).await;
        Ok(())
    }
    #[cfg(mobile)]
    {
        let _ = (app, service);
        Err(CommandError::new(
            "feature_unavailable",
            "In-app relaunch updates are available only on desktop.",
        ))
    }
}

#[tauri::command]
pub async fn set_server_configuration(
    service: State<'_, Arc<AgentService>>,
    input: OpenPrinterServerConfigurationInput,
) -> Result<OpenPrinterServerConfiguration, CommandError> {
    service.set_server_configuration(input).await
}

#[tauri::command]
pub async fn reset_server_configuration(
    service: State<'_, Arc<AgentService>>,
) -> Result<OpenPrinterServerConfiguration, CommandError> {
    service.reset_server_configuration().await
}

#[tauri::command]
pub async fn open_product_link(
    app: AppHandle,
    service: State<'_, Arc<AgentService>>,
    link: ProductLink,
) -> Result<(), CommandError> {
    let url = service.product_link_url(link)?;
    #[cfg(desktop)]
    {
        use oppa_platform::BrowserOpener as _;
        let _ = app;
        oppa_platform::SystemBrowser
            .open(&url)
            .map_err(|error| CommandError::new("browser_failed", error.to_string()))
    }
    #[cfg(mobile)]
    {
        crate::mobile::open_url(&app, &url).await
    }
}

/// Browses the local network for printer services (mobile only).
#[tauri::command]
pub async fn discover_network_printers(
    app: AppHandle,
    service: State<'_, Arc<AgentService>>,
) -> Result<Vec<DiscoveredNetworkPrinter>, CommandError> {
    if !service.capabilities.network_service_discovery
        || !service.product.features.network_printer_discovery
    {
        return Err(CommandError::new(
            "feature_unavailable",
            "Network printer discovery is not available on this platform.",
        ));
    }
    #[cfg(mobile)]
    {
        crate::mobile::discover_network_printers(&app, &service).await
    }
    #[cfg(desktop)]
    {
        let _ = app;
        desktop_only("Network printer discovery")
    }
}

/// Mobile lifecycle state, permissions, guidance, and limitations.
#[tauri::command]
pub async fn get_mobile_lifecycle(
    app: AppHandle,
    service: State<'_, Arc<AgentService>>,
) -> Result<MobileLifecycleStatus, CommandError> {
    #[cfg(mobile)]
    {
        crate::mobile::lifecycle_status(&app, &service).await
    }
    #[cfg(desktop)]
    {
        let _ = (app, service);
        desktop_only("Mobile lifecycle")
    }
}

#[tauri::command]
pub async fn set_background_agent(
    app: AppHandle,
    service: State<'_, Arc<AgentService>>,
    enabled: bool,
) -> Result<(), CommandError> {
    #[cfg(mobile)]
    {
        crate::mobile::set_background_agent(&app, service.inner(), enabled).await
    }
    #[cfg(desktop)]
    {
        let _ = (app, service, enabled);
        desktop_only("The background agent")
    }
}

#[tauri::command]
pub async fn set_keep_awake(
    app: AppHandle,
    service: State<'_, Arc<AgentService>>,
    enabled: bool,
) -> Result<(), CommandError> {
    #[cfg(mobile)]
    {
        crate::mobile::set_keep_awake(&app, service.inner(), enabled).await
    }
    #[cfg(desktop)]
    {
        let _ = (app, service, enabled);
        desktop_only("Keep-awake mode")
    }
}

#[tauri::command]
pub async fn request_notification_permission(app: AppHandle) -> Result<bool, CommandError> {
    #[cfg(mobile)]
    {
        crate::mobile::request_notification_permission(&app).await
    }
    #[cfg(desktop)]
    {
        let _ = app;
        desktop_only("Notification permission")
    }
}

#[tauri::command]
pub async fn request_battery_exemption(app: AppHandle) -> Result<(), CommandError> {
    #[cfg(mobile)]
    {
        crate::mobile::request_battery_exemption(&app).await
    }
    #[cfg(desktop)]
    {
        let _ = app;
        desktop_only("Battery optimization settings")
    }
}

/// Shares the sanitized diagnostics bundle (mobile only; desktop exports a file).
#[tauri::command]
pub async fn share_diagnostics(
    app: AppHandle,
    service: State<'_, Arc<AgentService>>,
) -> Result<(), CommandError> {
    #[cfg(mobile)]
    {
        crate::mobile::share_diagnostics(&app, &service).await
    }
    #[cfg(desktop)]
    {
        let _ = (app, service);
        desktop_only("Sharing diagnostics")
    }
}

#[tauri::command]
pub async fn open_battery_guide(app: AppHandle) -> Result<(), CommandError> {
    #[cfg(mobile)]
    {
        crate::mobile::open_battery_guide(&app).await
    }
    #[cfg(desktop)]
    {
        let _ = app;
        desktop_only("Battery guides")
    }
}

#[tauri::command]
pub async fn open_app_settings(app: AppHandle) -> Result<(), CommandError> {
    #[cfg(mobile)]
    {
        crate::mobile::open_app_settings(&app).await
    }
    #[cfg(desktop)]
    {
        let _ = app;
        desktop_only("App settings")
    }
}

#[tauri::command]
pub async fn request_local_network_access(app: AppHandle) -> Result<String, CommandError> {
    #[cfg(mobile)]
    {
        crate::mobile::request_local_network_access(&app).await
    }
    #[cfg(desktop)]
    {
        let _ = app;
        desktop_only("Local network permission")
    }
}

/// Checks the Android direct-download channel. Desktop uses the signed updater.
#[tauri::command]
pub async fn check_mobile_update(app: AppHandle) -> Result<Option<MobileUpdate>, CommandError> {
    #[cfg(mobile)]
    {
        crate::mobile::check_update(&app).await
    }
    #[cfg(desktop)]
    {
        let _ = app;
        desktop_only("APK updates")
    }
}

/// Downloads, verifies, and hands the offered APK to the system installer.
#[tauri::command]
pub async fn install_mobile_update(
    app: AppHandle,
    service: State<'_, Arc<AgentService>>,
) -> Result<String, CommandError> {
    #[cfg(mobile)]
    {
        let outcome = crate::mobile::install_update(&app, &service).await?;
        Ok(serde_json::to_value(outcome)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_default())
    }
    #[cfg(desktop)]
    {
        let _ = (app, service);
        desktop_only("APK updates")
    }
}

#[tauri::command]
pub async fn list_recent_servers(
    service: State<'_, Arc<AgentService>>,
) -> Result<Vec<RecentServer>, CommandError> {
    service.list_recent_servers().await
}

#[tauri::command]
pub async fn apply_recent_server(
    service: State<'_, Arc<AgentService>>,
    server_url: String,
) -> Result<(), CommandError> {
    service.apply_recent_server(server_url).await
}

#[tauri::command]
pub async fn delete_recent_server(
    service: State<'_, Arc<AgentService>>,
    server_url: String,
) -> Result<(), CommandError> {
    service.delete_recent_server(server_url).await
}

/// Fetches the server brand name from its discovery document without changing agent state.
///
/// Used by the deep-link dialog to show a human-readable name before the user confirms pairing.
/// Returns `None` on any network or parse failure so the UI can fall back to displaying the URL.
#[tauri::command]
pub async fn fetch_server_name(
    service: State<'_, Arc<AgentService>>,
    server_url: String,
) -> Result<Option<String>, ()> {
    Ok(service.fetch_server_name(server_url).await)
}

/// Returns and clears any deep-link pair request that arrived before the frontend was ready.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn get_pending_deep_link(state: State<'_, Arc<PendingDeepLink>>) -> Option<DeepLinkPayload> {
    state.0.lock().ok()?.take()
}

/// Simulates a deep-link URL arriving from the OS.
///
/// Active only in debug builds. In the webview console, call:
/// `globalThis.handleDeeplink("oppa-dev://pair?server=...&key=...")`
/// to exercise the full pairing flow without the OS URL handler.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn simulate_deep_link(app: AppHandle, url: String) {
    #[cfg(debug_assertions)]
    crate::dispatch_deep_link(&app, &url);
    #[cfg(not(debug_assertions))]
    let _ = (app, url);
}
