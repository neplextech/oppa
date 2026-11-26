//! Desktop-only shell integration: tray, application menu, and
//! close-to-background window handling.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use tauri::{
    App, AppHandle, Emitter, Listener, Manager,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
};

#[cfg(target_os = "macos")]
use tauri::menu::Submenu;

use crate::service::AgentService;

const OPEN_ID: &str = "open";
const PRINTERS_ID: &str = "printers";
const DIAGNOSTICS_ID: &str = "diagnostics";
const RECONNECT_ID: &str = "reconnect";
const QUIT_ID: &str = "quit";
const NAVIGATE_EVENT: &str = "oppa://navigate";

// App menu item IDs (prefixed to avoid collision with tray IDs)
const APP_HIDE_TO_TRAY: &str = "app-hide-to-tray";
const APP_QUIT: &str = "app-quit";
const APP_ADD_PRINTER: &str = "app-add-printer";
const APP_CLOSE_WINDOW: &str = "app-close-window";
const APP_NAV_OVERVIEW: &str = "app-nav-overview";
const APP_NAV_JOBS: &str = "app-nav-jobs";
const APP_NAV_PRINTERS: &str = "app-nav-printers";
const APP_NAV_VIRTUAL: &str = "app-nav-virtual";
const APP_NAV_LOGS: &str = "app-nav-logs";
const APP_NAV_SETTINGS: &str = "app-nav-settings";
const APP_RECONNECT: &str = "app-reconnect";
const APP_WINDOW_CENTER: &str = "app-window-center";

/// Explicit quit guard used by close-to-background handling.
pub struct QuitState(pub AtomicBool);

impl Default for QuitState {
    fn default() -> Self {
        Self(AtomicBool::new(false))
    }
}

/// Installs the system tray and, on macOS, the application menu.
pub fn setup(app: &App, service: &AgentService) {
    if let Err(error) = setup_tray(app) {
        service
            .log
            .warn("desktop", format!("System tray is unavailable: {error}"));
    }
    #[cfg(target_os = "macos")]
    if let Err(error) = setup_app_menu(app) {
        service
            .log
            .warn("desktop", format!("App menu is unavailable: {error}"));
    }
}

pub fn setup_tray(app: &App) -> tauri::Result<()> {
    let service = app.state::<Arc<AgentService>>();
    let product_name = service.product.product_name.as_str();
    let tray_id = format!("{}-tray", service.product.product_id);
    let open = MenuItem::with_id(
        app,
        OPEN_ID,
        format!("Open {product_name}"),
        true,
        None::<&str>,
    )?;
    let connection = MenuItem::with_id(
        app,
        "tray-connection",
        "Starting agent…",
        false,
        None::<&str>,
    )?;
    let workload = MenuItem::with_id(
        app,
        "tray-workload",
        "Loading printer status…",
        false,
        None::<&str>,
    )?;
    let jobs = MenuItem::with_id(app, "jobs", "Print jobs", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let hide = MenuItem::with_id(
        app,
        "hide-window",
        "Hide window (keep agent running)",
        true,
        None::<&str>,
    )?;
    let printers = MenuItem::with_id(app, PRINTERS_ID, "Printers", true, None::<&str>)?;
    let diagnostics = MenuItem::with_id(app, DIAGNOSTICS_ID, "Diagnostics", true, None::<&str>)?;
    let reconnect = MenuItem::with_id(app, RECONNECT_ID, "Reconnect", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, QUIT_ID, "Quit and stop agent", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &connection,
            &workload,
            &open,
            &jobs,
            &settings,
            &printers,
            &diagnostics,
            &reconnect,
            &separator,
            &hide,
            &quit,
        ],
    )?;
    let mut tray = TrayIconBuilder::with_id(tray_id.clone())
        .menu(&menu)
        .tooltip(product_name)
        // Both left and right click open the tray menu; the main window is
        // reached through the "Open" item.
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| handle_tray_menu(app, &event));
    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }
    tray.build(app)?;
    observe_tray_status(app, connection, workload, reconnect, tray_id);
    Ok(())
}

fn observe_tray_status(
    app: &App,
    connection: MenuItem<tauri::Wry>,
    workload: MenuItem<tauri::Wry>,
    reconnect: MenuItem<tauri::Wry>,
    tray_id: String,
) {
    let handle = app.handle().clone();
    let update = Arc::new(move || {
        let app = handle.clone();
        let connection = connection.clone();
        let workload = workload.clone();
        let reconnect = reconnect.clone();
        let tray_id = tray_id.clone();
        tauri::async_runtime::spawn(async move {
            let service = Arc::clone(app.state::<Arc<AgentService>>().inner());
            let status = service.status(false, String::new()).await;
            let (available, enabled) = service.catalog.availability_counts().await;
            let connected =
                status.connection_state == crate::models::OpenPrinterConnectionState::Connected;
            let state = match status.connection_state {
                crate::models::OpenPrinterConnectionState::Connected => "Connected",
                crate::models::OpenPrinterConnectionState::Connecting
                | crate::models::OpenPrinterConnectionState::Authenticating => "Connecting",
                crate::models::OpenPrinterConnectionState::CredentialRevoked
                | crate::models::OpenPrinterConnectionState::CredentialNotFound
                | crate::models::OpenPrinterConnectionState::AuthenticationFailed => {
                    "Pairing needs attention"
                }
                _ => "Not connected",
            };
            let identity = status.connected_service.map_or_else(
                || service.product.product_name.clone(),
                |server| server.name,
            );
            let summary = format!(
                "{} pending · {available}/{enabled} printers available",
                status.pending_jobs
            );
            let _ = connection.set_text(format!("{identity} · {state}"));
            let _ = workload.set_text(&summary);
            let _ = reconnect.set_enabled(status.agent_id.is_some());
            if let Some(tray) = app.tray_by_id(&tray_id) {
                let _ = tray.set_tooltip(Some(format!("{identity} · {state}\n{summary}")));
                if let Some(icon) = app.default_window_icon() {
                    let _ = tray.set_icon(Some(connection_icon(icon, connected)));
                }
            }
        });
    });
    for event in [
        crate::service::STATE_CHANGED_EVENT,
        crate::service::JOBS_CHANGED_EVENT,
        crate::service::PRINTERS_CHANGED_EVENT,
    ] {
        let update = Arc::clone(&update);
        app.listen(event, move |_| update());
    }
    update();
}

fn connection_icon(
    original: &tauri::image::Image<'_>,
    connected: bool,
) -> tauri::image::Image<'static> {
    let (width, height) = (original.width(), original.height());
    let mut pixels = original.rgba().to_vec();
    let radius = (width.min(height) / 6).max(1);
    let (cx, cy) = (
        width.saturating_sub(radius + 1),
        height.saturating_sub(radius + 1),
    );
    for y in 0..height {
        for x in 0..width {
            let dx = i64::from(x) - i64::from(cx);
            let dy = i64::from(y) - i64::from(cy);
            if dx * dx + dy * dy <= i64::from(radius) * i64::from(radius) {
                let offset = ((y * width + x) * 4) as usize;
                let color = if connected {
                    [93, 175, 126, 255]
                } else {
                    [224, 164, 69, 255]
                };
                pixels[offset..offset + 4].copy_from_slice(&color);
            }
        }
    }
    tauri::image::Image::new_owned(pixels, width, height)
}

pub fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Hides the main window while the tray agent keeps running.
fn hide_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

/// Shuts the agent down gracefully and exits the process.
pub(crate) fn quit_application(app: &AppHandle) {
    app.state::<QuitState>().0.store(true, Ordering::Release);
    let service = Arc::clone(app.state::<Arc<AgentService>>().inner());
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        service.shutdown().await;
        app.exit(0);
    });
}

pub fn handle_close_request(window: &tauri::Window, event: &tauri::WindowEvent) {
    let tauri::WindowEvent::CloseRequested { api, .. } = event else {
        return;
    };
    let quitting = window
        .app_handle()
        .state::<QuitState>()
        .0
        .load(Ordering::Acquire);
    if !quitting {
        api.prevent_close();
        let _ = window.hide();
    }
}

fn handle_tray_menu(app: &AppHandle, event: &tauri::menu::MenuEvent) {
    match event.id().as_ref() {
        OPEN_ID => {
            show_main_window(app);
            let _ = app.emit(NAVIGATE_EVENT, "overview");
        }
        "hide-window" => hide_main_window(app),
        "jobs" | "settings" => {
            show_main_window(app);
            let _ = app.emit(NAVIGATE_EVENT, event.id().as_ref());
        }
        PRINTERS_ID => {
            show_main_window(app);
            let _ = app.emit(NAVIGATE_EVENT, "printers");
        }
        DIAGNOSTICS_ID => {
            show_main_window(app);
            let _ = app.emit(NAVIGATE_EVENT, "diagnostics");
        }
        RECONNECT_ID => {
            let service = Arc::clone(app.state::<Arc<AgentService>>().inner());
            tauri::async_runtime::spawn(async move {
                if let Err(error) = service.reconnect().await {
                    service.log.warn("transport", error.to_string());
                }
            });
        }
        QUIT_ID => quit_application(app),
        _ => {}
    }
}

#[cfg(target_os = "macos")]
pub fn setup_app_menu(app: &App) -> tauri::Result<()> {
    let menu = build_app_menu(app)?;
    app.set_menu(menu)?;
    Ok(())
}

/// Builds the first submenu, which macOS renders as the application menu.
/// Cmd+Q hides to the tray instead of terminating so the background agent
/// keeps running; only the explicit Quit item stops the process.
#[cfg(target_os = "macos")]
fn build_application_submenu(app: &App, product_name: &str) -> tauri::Result<Submenu<tauri::Wry>> {
    let hide_to_tray = MenuItem::with_id(
        app,
        APP_HIDE_TO_TRAY,
        "Hide to Tray",
        true,
        Some("CmdOrCtrl+Q"),
    )?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(
        app,
        APP_QUIT,
        format!("Quit {product_name}"),
        true,
        None::<&str>,
    )?;
    Submenu::with_items(app, product_name, true, &[&hide_to_tray, &separator, &quit])
}

#[cfg(target_os = "macos")]
fn build_app_menu(app: &App) -> tauri::Result<Menu<tauri::Wry>> {
    let product_name = app.try_state::<Arc<AgentService>>().map_or_else(
        || "OPPA".to_owned(),
        |service| service.product.product_name.clone(),
    );
    let menu = Menu::new(app)?;

    menu.append(&build_application_submenu(app, &product_name)?)?;
    // File
    let add_printer = MenuItem::with_id(
        app,
        APP_ADD_PRINTER,
        "Add Network Printer…",
        true,
        None::<&str>,
    )?;
    let file_sep = PredefinedMenuItem::separator(app)?;
    let close_window = MenuItem::with_id(
        app,
        APP_CLOSE_WINDOW,
        "Close Window",
        true,
        Some("CmdOrCtrl+W"),
    )?;
    let file_menu =
        Submenu::with_items(app, "File", true, &[&add_printer, &file_sep, &close_window])?;
    menu.append(&file_menu)?;

    // View
    let nav_overview =
        MenuItem::with_id(app, APP_NAV_OVERVIEW, "Overview", true, Some("CmdOrCtrl+1"))?;
    let nav_jobs = MenuItem::with_id(app, APP_NAV_JOBS, "Jobs", true, Some("CmdOrCtrl+2"))?;
    let nav_printers =
        MenuItem::with_id(app, APP_NAV_PRINTERS, "Printers", true, Some("CmdOrCtrl+3"))?;
    let nav_virtual = MenuItem::with_id(
        app,
        APP_NAV_VIRTUAL,
        "Virtual Printer",
        true,
        Some("CmdOrCtrl+4"),
    )?;
    let nav_logs = MenuItem::with_id(app, APP_NAV_LOGS, "Logs", true, Some("CmdOrCtrl+5"))?;
    let view_sep = PredefinedMenuItem::separator(app)?;
    let nav_settings =
        MenuItem::with_id(app, APP_NAV_SETTINGS, "Settings", true, Some("CmdOrCtrl+,"))?;
    let view_menu = Submenu::with_items(
        app,
        "View",
        true,
        &[
            &nav_overview,
            &nav_jobs,
            &nav_printers,
            &nav_virtual,
            &nav_logs,
            &view_sep,
            &nav_settings,
        ],
    )?;
    menu.append(&view_menu)?;

    // Edit — enables standard keyboard shortcuts (Cmd+C/V/X/Z/A) in the webview
    let edit_undo = PredefinedMenuItem::undo(app, None)?;
    let edit_redo = PredefinedMenuItem::redo(app, None)?;
    let edit_sep1 = PredefinedMenuItem::separator(app)?;
    let edit_cut = PredefinedMenuItem::cut(app, None)?;
    let edit_copy = PredefinedMenuItem::copy(app, None)?;
    let edit_paste = PredefinedMenuItem::paste(app, None)?;
    let edit_sep2 = PredefinedMenuItem::separator(app)?;
    let edit_select_all = PredefinedMenuItem::select_all(app, None)?;
    let edit_menu = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &edit_undo,
            &edit_redo,
            &edit_sep1,
            &edit_cut,
            &edit_copy,
            &edit_paste,
            &edit_sep2,
            &edit_select_all,
        ],
    )?;
    menu.append(&edit_menu)?;

    // Agent
    let reconnect = MenuItem::with_id(app, APP_RECONNECT, "Reconnect", true, None::<&str>)?;
    let agent_menu = Submenu::with_items(app, "Agent", true, &[&reconnect])?;
    menu.append(&agent_menu)?;

    // Window
    let window_submenu = Submenu::new(app, "Window", true)?;
    window_submenu.append(&PredefinedMenuItem::minimize(app, None)?)?;
    window_submenu.append(&PredefinedMenuItem::maximize(app, None)?)?;
    window_submenu.append(&PredefinedMenuItem::fullscreen(app, None)?)?;
    window_submenu.append(&PredefinedMenuItem::separator(app)?)?;
    window_submenu.append(&MenuItem::with_id(
        app,
        APP_WINDOW_CENTER,
        "Center Window",
        true,
        None::<&str>,
    )?)?;
    window_submenu.append(&PredefinedMenuItem::bring_all_to_front(app, None)?)?;
    menu.append(&window_submenu)?;

    Ok(menu)
}

pub fn handle_app_menu_event(app: &AppHandle, event: &tauri::menu::MenuEvent) {
    match event.id().as_ref() {
        APP_CLOSE_WINDOW | APP_HIDE_TO_TRAY => hide_main_window(app),
        APP_QUIT => quit_application(app),
        APP_NAV_OVERVIEW => {
            show_main_window(app);
            let _ = app.emit(NAVIGATE_EVENT, "overview");
        }
        APP_NAV_JOBS => {
            show_main_window(app);
            let _ = app.emit(NAVIGATE_EVENT, "jobs");
        }
        APP_ADD_PRINTER | APP_NAV_PRINTERS => {
            show_main_window(app);
            let _ = app.emit(NAVIGATE_EVENT, "printers");
        }
        APP_NAV_VIRTUAL => {
            show_main_window(app);
            let _ = app.emit(NAVIGATE_EVENT, "virtual");
        }
        APP_NAV_LOGS => {
            show_main_window(app);
            let _ = app.emit(NAVIGATE_EVENT, "diagnostics");
        }
        APP_NAV_SETTINGS => {
            show_main_window(app);
            let _ = app.emit(NAVIGATE_EVENT, "settings");
        }
        APP_RECONNECT => {
            let service = Arc::clone(app.state::<Arc<AgentService>>().inner());
            tauri::async_runtime::spawn(async move {
                if let Err(error) = service.reconnect().await {
                    service.log.warn("transport", error.to_string());
                }
            });
        }
        APP_WINDOW_CENTER => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.center();
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::QuitState;

    #[test]
    fn tray_connection_overlay_preserves_product_icon_geometry() {
        let original = tauri::image::Image::new_owned(vec![0; 16 * 16 * 4], 16, 16);
        let connected = super::connection_icon(&original, true);
        let disconnected = super::connection_icon(&original, false);
        assert_eq!((connected.width(), connected.height()), (16, 16));
        assert_ne!(connected.rgba(), disconnected.rgba());
        assert_eq!(&connected.rgba()[..4], &original.rgba()[..4]);
    }

    #[test]
    fn quit_state_defaults_to_background_mode() {
        assert!(!QuitState::default().0.load(Ordering::Relaxed));
    }
}
