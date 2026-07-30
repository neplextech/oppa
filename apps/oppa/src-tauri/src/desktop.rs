use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use tauri::{
    App, AppHandle, Emitter, Manager,
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

use crate::service::DesktopService;

const OPEN_ID: &str = "open";
const PRINTERS_ID: &str = "printers";
const DIAGNOSTICS_ID: &str = "diagnostics";
const RECONNECT_ID: &str = "reconnect";
const QUIT_ID: &str = "quit";
const NAVIGATE_EVENT: &str = "oppa://navigate";

// App menu item IDs (prefixed to avoid collision with tray IDs)
const APP_ADD_PRINTER: &str = "app-add-printer";
const APP_CLOSE_WINDOW: &str = "app-close-window";
const APP_NAV_OVERVIEW: &str = "app-nav-overview";
const APP_NAV_JOBS: &str = "app-nav-jobs";
const APP_NAV_PRINTERS: &str = "app-nav-printers";
const APP_NAV_VIRTUAL: &str = "app-nav-virtual";
const APP_NAV_LOGS: &str = "app-nav-logs";
const APP_NAV_SETTINGS: &str = "app-nav-settings";
const APP_RECONNECT: &str = "app-reconnect";

/// Explicit quit guard used by close-to-background handling.
pub struct QuitState(pub AtomicBool);

impl Default for QuitState {
    fn default() -> Self {
        Self(AtomicBool::new(false))
    }
}

pub fn setup_tray(app: &App) -> tauri::Result<()> {
    let service = app.state::<Arc<DesktopService>>();
    let product_name = service.product.product_name.as_str();
    let tray_id = format!("{}-tray", service.product.product_id);
    let open = MenuItem::with_id(
        app,
        OPEN_ID,
        format!("Open {product_name}"),
        true,
        None::<&str>,
    )?;
    let printers = MenuItem::with_id(app, PRINTERS_ID, "Printers", true, None::<&str>)?;
    let diagnostics = MenuItem::with_id(app, DIAGNOSTICS_ID, "Diagnostics", true, None::<&str>)?;
    let reconnect = MenuItem::with_id(app, RECONNECT_ID, "Reconnect", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, QUIT_ID, "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &open,
            &printers,
            &diagnostics,
            &reconnect,
            &separator,
            &quit,
        ],
    )?;
    let mut tray = TrayIconBuilder::with_id(tray_id)
        .menu(&menu)
        .tooltip(product_name)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| handle_tray_menu(app, &event))
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } | TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                }
            ) {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }
    tray.build(app)?;
    Ok(())
}

pub fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
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
        OPEN_ID => show_main_window(app),
        PRINTERS_ID => {
            show_main_window(app);
            let _ = app.emit(NAVIGATE_EVENT, "printers");
        }
        DIAGNOSTICS_ID => {
            show_main_window(app);
            let _ = app.emit(NAVIGATE_EVENT, "diagnostics");
        }
        RECONNECT_ID => {
            let service = Arc::clone(app.state::<Arc<DesktopService>>().inner());
            tauri::async_runtime::spawn(async move {
                if let Err(error) = service.reconnect().await {
                    service.log.warn("transport", error.to_string());
                }
            });
        }
        QUIT_ID => {
            app.state::<QuitState>().0.store(true, Ordering::Release);
            let service = Arc::clone(app.state::<Arc<DesktopService>>().inner());
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                service.shutdown().await;
                app.exit(0);
            });
        }
        _ => {}
    }
}

pub fn setup_app_menu(app: &App) -> tauri::Result<()> {
    let menu = build_app_menu(app)?;
    app.set_menu(menu)?;
    Ok(())
}

fn build_app_menu(app: &App) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;

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

    // Agent
    let reconnect = MenuItem::with_id(app, APP_RECONNECT, "Reconnect", true, None::<&str>)?;
    let agent_menu = Submenu::with_items(app, "Agent", true, &[&reconnect])?;
    menu.append(&agent_menu)?;

    Ok(menu)
}

pub fn handle_app_menu_event(app: &AppHandle, event: &tauri::menu::MenuEvent) {
    match event.id().as_ref() {
        APP_CLOSE_WINDOW => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide();
            }
        }
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
            let service = Arc::clone(app.state::<Arc<DesktopService>>().inner());
            tauri::async_runtime::spawn(async move {
                if let Err(error) = service.reconnect().await {
                    service.log.warn("transport", error.to_string());
                }
            });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::QuitState;

    #[test]
    fn quit_state_defaults_to_background_mode() {
        assert!(!QuitState::default().0.load(Ordering::Relaxed));
    }
}
