use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use tauri::{
    App, AppHandle, Emitter, Manager,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

use crate::service::DesktopService;

const OPEN_ID: &str = "open";
const PRINTERS_ID: &str = "printers";
const DIAGNOSTICS_ID: &str = "diagnostics";
const RECONNECT_ID: &str = "reconnect";
const QUIT_ID: &str = "quit";
const NAVIGATE_EVENT: &str = "oppa://navigate";

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

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::QuitState;

    #[test]
    fn quit_state_defaults_to_background_mode() {
        assert!(!QuitState::default().0.load(Ordering::Relaxed));
    }
}
