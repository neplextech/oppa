use std::sync::Arc;

use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;

use crate::{
    error::CommandError,
    models::{
        AgentStatus, ConfigurePrinterChanges, DeepLinkPayload, Diagnostics,
        DiscoveredServiceSummary, JobSummary, ManualPrinterInput, PendingDeepLink, PrinterSummary,
        ProductLink, RecentServer, VirtualPrinterInput, VirtualPrinterMode,
    },
    server_configuration::{OpenPrinterServerConfiguration, OpenPrinterServerConfigurationInput},
    service::{DesktopService, STATE_CHANGED_EVENT},
};

#[tauri::command]
pub async fn get_agent_status(
    app: AppHandle,
    service: State<'_, Arc<DesktopService>>,
) -> Result<AgentStatus, CommandError> {
    let start_on_login = app.autolaunch().is_enabled().unwrap_or(false);
    let version = app.package_info().version.to_string();
    Ok(service.status(start_on_login, version).await)
}

#[tauri::command]
pub async fn discover_server(
    service: State<'_, Arc<DesktopService>>,
) -> Result<DiscoveredServiceSummary, CommandError> {
    service.discover_server().await
}

#[tauri::command]
pub async fn pair_server(
    service: State<'_, Arc<DesktopService>>,
    code: String,
    agent_name: String,
) -> Result<DiscoveredServiceSummary, CommandError> {
    service.pair_server(code, agent_name).await
}

#[tauri::command]
pub async fn forget_server(service: State<'_, Arc<DesktopService>>) -> Result<(), CommandError> {
    service.forget_server().await
}

#[tauri::command]
pub async fn list_printers(
    service: State<'_, Arc<DesktopService>>,
) -> Result<Vec<PrinterSummary>, CommandError> {
    service.list_printers().await
}

#[tauri::command]
pub async fn refresh_printers(
    service: State<'_, Arc<DesktopService>>,
) -> Result<Vec<PrinterSummary>, CommandError> {
    service.refresh_printers().await
}

#[tauri::command]
pub async fn configure_printer(
    service: State<'_, Arc<DesktopService>>,
    printer_id: String,
    changes: ConfigurePrinterChanges,
) -> Result<PrinterSummary, CommandError> {
    service.configure_printer(&printer_id, changes).await
}

#[tauri::command]
pub async fn add_manual_printer(
    service: State<'_, Arc<DesktopService>>,
    input: ManualPrinterInput,
) -> Result<PrinterSummary, CommandError> {
    service.add_manual_printer(input).await
}

#[tauri::command]
pub async fn remove_printer(
    service: State<'_, Arc<DesktopService>>,
    printer_id: String,
) -> Result<(), CommandError> {
    service.remove_printer(&printer_id).await
}

#[tauri::command]
pub async fn create_virtual_printer(
    service: State<'_, Arc<DesktopService>>,
    input: VirtualPrinterInput,
) -> Result<PrinterSummary, CommandError> {
    service.create_virtual_printer(input).await
}

#[tauri::command]
pub async fn update_virtual_printer(
    service: State<'_, Arc<DesktopService>>,
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
    service: State<'_, Arc<DesktopService>>,
    printer_id: String,
) -> Result<(), CommandError> {
    service.clear_virtual_history(&printer_id).await
}

#[tauri::command]
pub async fn set_virtual_printer_sound(
    service: State<'_, Arc<DesktopService>>,
    printer_id: String,
    enabled: bool,
) -> Result<(), CommandError> {
    service
        .set_virtual_printer_sound(&printer_id, enabled)
        .await
}

#[tauri::command]
pub async fn send_test_print(
    service: State<'_, Arc<DesktopService>>,
    printer_id: String,
) -> Result<JobSummary, CommandError> {
    service.send_test_print(&printer_id).await
}

#[tauri::command]
pub async fn list_recent_jobs(
    service: State<'_, Arc<DesktopService>>,
) -> Result<Vec<JobSummary>, CommandError> {
    Ok(service.list_recent_jobs().await)
}

#[tauri::command]
pub async fn clear_jobs(service: State<'_, Arc<DesktopService>>) -> Result<(), CommandError> {
    service.clear_jobs().await
}

#[tauri::command]
pub async fn clear_logs(service: State<'_, Arc<DesktopService>>) -> Result<(), CommandError> {
    service.clear_logs();
    Ok(())
}

#[tauri::command]
pub async fn get_diagnostics(
    service: State<'_, Arc<DesktopService>>,
) -> Result<Diagnostics, CommandError> {
    Ok(service.diagnostics().await)
}

#[tauri::command]
pub async fn export_diagnostics(
    service: State<'_, Arc<DesktopService>>,
) -> Result<String, CommandError> {
    service.export_diagnostics().await
}

#[tauri::command]
pub async fn set_start_on_login(
    app: AppHandle,
    service: State<'_, Arc<DesktopService>>,
    enabled: bool,
) -> Result<bool, CommandError> {
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
    service.emit(STATE_CHANGED_EVENT);
    Ok(actual)
}

#[tauri::command]
pub async fn reconnect(
    app: AppHandle,
    service: State<'_, Arc<DesktopService>>,
) -> Result<AgentStatus, CommandError> {
    service.reconnect().await?;
    tokio::task::yield_now().await;
    let start_on_login = app.autolaunch().is_enabled().unwrap_or(false);
    let version = app.package_info().version.to_string();
    Ok(service.status(start_on_login, version).await)
}

#[tauri::command]
pub async fn set_server_configuration(
    service: State<'_, Arc<DesktopService>>,
    input: OpenPrinterServerConfigurationInput,
) -> Result<OpenPrinterServerConfiguration, CommandError> {
    service.set_server_configuration(input).await
}

#[tauri::command]
pub async fn reset_server_configuration(
    service: State<'_, Arc<DesktopService>>,
) -> Result<OpenPrinterServerConfiguration, CommandError> {
    service.reset_server_configuration().await
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn open_product_link(
    service: State<'_, Arc<DesktopService>>,
    link: ProductLink,
) -> Result<(), CommandError> {
    service.open_product_link(link)
}

#[tauri::command]
pub async fn list_recent_servers(
    service: State<'_, Arc<DesktopService>>,
) -> Result<Vec<RecentServer>, CommandError> {
    service.list_recent_servers().await
}

#[tauri::command]
pub async fn apply_recent_server(
    service: State<'_, Arc<DesktopService>>,
    server_url: String,
) -> Result<(), CommandError> {
    service.apply_recent_server(server_url).await
}

#[tauri::command]
pub async fn delete_recent_server(
    service: State<'_, Arc<DesktopService>>,
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
    service: State<'_, Arc<DesktopService>>,
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
