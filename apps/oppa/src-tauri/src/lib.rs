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

use desktop::QuitState;
use service::DesktopService;
use tauri::Manager;
use tauri_plugin_autostart::MacosLauncher;

/// Starts the OPPA desktop process and its background agent runtime.
///
/// # Panics
///
/// Panics when Tauri cannot start the application process.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_single_instance::init(
            |app, _arguments, _cwd| {
                desktop::show_main_window(app);
            },
        ))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(QuitState::default())
        .setup(|app| {
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
        ])
        .run(tauri::generate_context!())
        .expect("unrecoverable Tauri application failure");
}
