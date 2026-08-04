//! Production Tauri host for the shell-independent OPPA agent.

#![forbid(unsafe_code)]

mod commands;
mod connection;
mod desktop;
mod diagnostics;
mod error;
mod job_ledger;
mod models;
mod printer_catalog;
mod server_configuration;
mod service;
mod virtual_spooler;

use std::sync::Arc;

use base64::Engine as _;
use desktop::QuitState;
use models::{DeepLinkPayload, PendingDeepLink};
use service::DesktopService;
use tauri::{Emitter, Listener, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_autostart::MacosLauncher;

/// Starts the OPPA desktop process and its background agent runtime.
///
/// # Panics
///
/// Panics when Tauri cannot start the application process.
#[allow(clippy::too_many_lines)]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_single_instance::init(
            |app, arguments, _cwd| {
                desktop::show_main_window(app);

                for raw_url in arguments {
                    if let Some(payload) = parse_deep_link_pair(raw_url.as_str()) {
                        if let Some(state) = app.try_state::<Arc<PendingDeepLink>>() {
                            if let Ok(mut guard) = state.0.lock() {
                                *guard = Some(payload.clone());
                            }
                        }
                        let _ = app.emit("oppa://deep-link", payload);
                    }
                }
            },
        ))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(QuitState::default())
        .manage(Arc::new(PendingDeepLink::default()))
        .setup(|app| {
            // Create the main window in Rust so that platform-specific title bar
            // configuration is applied before the webview loads, which is required
            // for data-tauri-drag-region to work correctly on macOS.
            let product_name = oppa_product::embedded_product()
                .ok()
                .map(|p| p.product_name.clone())
                .unwrap_or_else(|| "OPPA".to_owned());
            let win_builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                .title(product_name.as_str())
                .inner_size(1120.0, 760.0)
                .min_inner_size(840.0, 600.0)
                .center();

            #[cfg(target_os = "macos")]
            let win_builder = win_builder
                .title_bar_style(tauri::TitleBarStyle::Overlay)
                .hidden_title(true);

            #[cfg(not(target_os = "macos"))]
            let win_builder = win_builder.decorations(false);

            win_builder.build()?;

            #[cfg(debug_assertions)]
            if let Some(window) = app.get_webview_window("main") {
                window.open_devtools();
            }

            let service =
                tauri::async_runtime::block_on(DesktopService::initialize(app.handle().clone()))?;
            app.manage(Arc::clone(&service));
            if let Err(error) = desktop::setup_tray(app) {
                service
                    .log
                    .warn("desktop", format!("System tray is unavailable: {error}"));
            }
            if let Err(error) = desktop::setup_app_menu(app) {
                service
                    .log
                    .warn("desktop", format!("App menu is unavailable: {error}"));
            }

            // Listen for deep-link URLs arriving while the app is already running.
            let dl_handle = app.handle().clone();
            app.listen("deep-link://new-url", move |event| {
                let urls: Vec<String> = match serde_json::from_str(event.payload()) {
                    Ok(u) => u,
                    Err(_) => return,
                };
                for raw_url in urls {
                    if let Some(payload) = parse_deep_link_pair(&raw_url) {
                        if let Some(state) = dl_handle.try_state::<Arc<PendingDeepLink>>() {
                            if let Ok(mut guard) = state.0.lock() {
                                *guard = Some(payload.clone());
                            }
                        }
                        let _ = dl_handle.emit("oppa://deep-link", payload);
                    }
                }
            });

            Ok(())
        })
        .on_menu_event(|app, event| desktop::handle_app_menu_event(app, &event))
        .on_window_event(desktop::handle_close_request)
        .invoke_handler(tauri::generate_handler![
            commands::get_agent_status,
            commands::discover_server,
            commands::pair_server,
            commands::forget_server,
            commands::list_printers,
            commands::refresh_printers,
            commands::configure_printer,
            commands::add_manual_printer,
            commands::remove_printer,
            commands::create_virtual_printer,
            commands::update_virtual_printer,
            commands::clear_virtual_history,
            commands::set_virtual_printer_sound,
            commands::send_test_print,
            commands::list_recent_jobs,
            commands::clear_jobs,
            commands::clear_logs,
            commands::get_diagnostics,
            commands::export_diagnostics,
            commands::set_start_on_login,
            commands::reconnect,
            commands::set_server_configuration,
            commands::reset_server_configuration,
            commands::open_product_link,
            commands::list_recent_servers,
            commands::apply_recent_server,
            commands::delete_recent_server,
            commands::get_pending_deep_link,
        ])
        .run(tauri::generate_context!())
        .expect("unrecoverable Tauri application failure");
}

/// Parses an `<scheme>://pair?server=<base64url>&key=<code>` deep link.
fn parse_deep_link_pair(raw_url: &str) -> Option<DeepLinkPayload> {
    let parsed = raw_url.parse::<url::Url>().ok()?;
    if parsed.scheme() != oppa_product::DEEP_LINK_SCHEME {
        return None;
    }
    if parsed.path().trim_matches('/') != "pair" {
        return None;
    }
    let params: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();
    let server_b64 = params.get("server")?;
    let pair_key = params.get("key")?;
    let server_bytes = base64::prelude::BASE64_URL_SAFE_NO_PAD
        .decode(server_b64)
        .ok()?;
    let server_url = String::from_utf8(server_bytes).ok()?;
    Some(DeepLinkPayload {
        server_url,
        pair_key: pair_key.clone(),
    })
}
