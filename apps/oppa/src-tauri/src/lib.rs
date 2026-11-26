//! Production Tauri host for the shell-independent OPPA agent.
//!
//! One host serves desktop (Windows, macOS, Linux) and mobile (Android, iOS).
//! Platform-specific behavior is selected at compile time: the tray, window
//! chrome, application menu, start-on-login, single-instance guard, and signed
//! updater exist only on desktop; the foreground service, Keystore bridge,
//! lifecycle handling, and APK update flow exist only on mobile.

#![forbid(unsafe_code)]

mod commands;
mod connection;
#[cfg(desktop)]
mod desktop;
mod diagnostics;
mod documents;
mod error;
mod job_ledger;
mod lifecycle;
#[cfg(mobile)]
mod mobile;
mod mobile_settings;
mod models;
mod network_discovery;
mod platform;
mod printer_catalog;
mod runtime_owner;
mod server_configuration;
mod service;
mod updates;
mod virtual_spooler;

use std::sync::Arc;

use base64::Engine as _;
use models::{DeepLinkPayload, PendingDeepLink};
use platform::PlatformCapabilities;
use runtime_owner::{Ownership, RuntimeSlot};
use service::{AgentService, HostEnvironment};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

/// The single agent runtime owned by this process.
static AGENT_RUNTIME: RuntimeSlot<AgentService> = RuntimeSlot::new();

/// Starts the OPPA process and its background agent runtime.
///
/// # Panics
///
/// Panics when Tauri cannot start the application process.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default().plugin(tauri_plugin_deep_link::init());
    #[cfg(desktop)]
    let builder = desktop_plugins(builder);
    #[cfg(mobile)]
    let builder = builder.plugin(tauri_plugin_oppa_mobile::init());

    let builder = builder
        .manage(Arc::new(PendingDeepLink::default()))
        .setup(|app| {
            create_main_window(app)?;

            let service = tauri::async_runtime::block_on(acquire_runtime(app.handle()))?;
            app.manage(Arc::clone(&service));

            #[cfg(desktop)]
            desktop::setup(app, &service);
            #[cfg(mobile)]
            mobile::setup(app, &service);

            register_deep_links(app);
            Ok(())
        });

    #[cfg(desktop)]
    let builder = builder
        .manage(desktop::QuitState::default())
        .on_menu_event(|app, event| desktop::handle_app_menu_event(app, &event))
        .on_window_event(desktop::handle_close_request);

    builder
        .invoke_handler(tauri::generate_handler![
            commands::simulate_deep_link,
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
            commands::send_test_pdf,
            commands::list_recent_jobs,
            commands::clear_jobs,
            commands::clear_logs,
            commands::get_diagnostics,
            commands::export_diagnostics,
            commands::get_diagnostics_bundle,
            commands::share_diagnostics,
            commands::set_start_on_login,
            commands::reconnect,
            commands::quit_application,
            commands::set_server_configuration,
            commands::reset_server_configuration,
            commands::open_product_link,
            commands::list_recent_servers,
            commands::apply_recent_server,
            commands::delete_recent_server,
            commands::get_pending_deep_link,
            commands::fetch_server_name,
            commands::discover_network_printers,
            commands::get_mobile_lifecycle,
            commands::set_background_agent,
            commands::set_keep_awake,
            commands::request_notification_permission,
            commands::request_battery_exemption,
            commands::open_app_settings,
            commands::open_battery_guide,
            commands::request_local_network_access,
            commands::check_mobile_update,
            commands::install_mobile_update,
        ])
        .build(tauri::generate_context!())
        .expect("unrecoverable Tauri application failure")
        .run(|app, event| {
            #[cfg(mobile)]
            mobile::handle_run_event(app, &event);
            #[cfg(desktop)]
            let _ = (app, event);
        });
}

#[cfg(desktop)]
fn desktop_plugins(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    use tauri_plugin_autostart::MacosLauncher;

    builder
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(
            tauri_plugin_frame::FramePluginBuilder::new()
                .titlebar_height(44)
                .button_width(46)
                .auto_titlebar(true)
                .snap_overlay(true)
                .build(),
        )
        .plugin(tauri_plugin_single_instance::init(
            |app, arguments, _cwd| {
                desktop::show_main_window(app);
                for raw_url in arguments {
                    dispatch_deep_link(app, raw_url.as_str());
                }
            },
        ))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
}

/// Creates the main window in Rust so platform chrome is applied before the
/// webview loads. macOS uses its native overlay title bar; Windows uses the
/// frame plugin over an undecorated window; Linux uses the custom webview
/// title bar. Mobile windows fill the screen and carry no window chrome.
///
/// A no-op while the window exists. On Android, Tauri drops the window when
/// its activity is destroyed (for example when the task is swiped away while
/// the foreground service keeps the process alive), so the mobile host calls
/// this again for the recreated activity.
pub(crate) fn create_main_window<M: Manager<tauri::Wry>>(app: &M) -> tauri::Result<()> {
    let product_name = oppa_product::embedded_product()
        .ok()
        .map_or_else(|| "OPPA".to_owned(), |p| p.product_name.clone());
    if app.get_webview_window("main").is_some() {
        return Ok(());
    }
    let builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::default());

    #[cfg(desktop)]
    let builder = {
        let builder = builder
            .title(product_name.as_str())
            .inner_size(1120.0, 760.0)
            .min_inner_size(800.0, 600.0)
            .center();
        #[cfg(target_os = "macos")]
        let builder = builder
            .title_bar_style(tauri::TitleBarStyle::Overlay)
            .hidden_title(true);
        #[cfg(not(target_os = "macos"))]
        let builder = builder.decorations(false);
        builder
    };
    #[cfg(mobile)]
    let _ = product_name;

    builder.build()?;

    #[cfg(all(debug_assertions, desktop))]
    if let Some(window) = app.get_webview_window("main") {
        window.open_devtools();
    }
    Ok(())
}

/// Returns the process-wide agent, creating and starting it exactly once.
async fn acquire_runtime(app: &tauri::AppHandle) -> Result<Arc<AgentService>, error::CommandError> {
    let (service, ownership) = AGENT_RUNTIME
        .acquire(|| async {
            let host = host_environment(app)?;
            AgentService::initialize(app.clone(), host).await
        })
        .await?;
    if ownership == Ownership::Reused {
        service
            .log
            .info("application", "Reattached to the running agent runtime.");
    }
    // A no-op when already started, so a reattaching host never starts a
    // second supervisor or job processor.
    service.start();
    Ok(service)
}

fn host_environment(app: &tauri::AppHandle) -> Result<HostEnvironment, error::CommandError> {
    let product = oppa_product::embedded_product()
        .map_err(|error| error::CommandError::internal(error.to_string()))?;
    let capabilities = PlatformCapabilities::current();

    #[cfg(desktop)]
    let (paths, credential_store) = {
        let _ = app;
        let paths = service::resolve_desktop_paths(product)?;
        let store: Arc<dyn oppa_platform::CredentialStore> = Arc::new(
            oppa_platform::KeyringCredentialStore::new(product.application_id.clone())
                .map_err(|error| error::CommandError::internal(error.to_string()))?,
        );
        (paths, store)
    };

    #[cfg(mobile)]
    let (paths, credential_store) = {
        let paths = mobile::resolve_paths(app)?;
        let store = mobile::credential_store(app, product)?;
        (paths, store)
    };

    Ok(HostEnvironment {
        paths,
        credential_store,
        capabilities,
    })
}

/// Routes deep links from every launch path through [`dispatch_deep_link`].
///
/// Covers warm links (`deep-link://new-url` events on every platform) and cold
/// links that launched the app (`get_current`, used on mobile where the URL is
/// not passed as a command-line argument).
fn register_deep_links(app: &tauri::App) {
    use tauri::Listener as _;
    use tauri_plugin_deep_link::DeepLinkExt as _;

    let handle = app.handle().clone();
    app.listen("deep-link://new-url", move |event| {
        let urls: Vec<String> = match serde_json::from_str(event.payload()) {
            Ok(urls) => urls,
            Err(_) => return,
        };
        for raw_url in urls {
            dispatch_deep_link(&handle, &raw_url);
        }
    });

    if let Ok(Some(urls)) = app.deep_link().get_current() {
        for url in urls {
            dispatch_deep_link(app.handle(), url.as_str());
        }
    }
}

/// Parses and dispatches a deep link URL through the normal pair flow.
///
/// Updates `PendingDeepLink` state and emits `oppa://deep-link` when the URL
/// matches the embedded scheme and the `pair` path. No-ops on any other URL.
pub(crate) fn dispatch_deep_link(app: &tauri::AppHandle, raw_url: &str) {
    if let Some(payload) = parse_deep_link_pair(raw_url) {
        if let Some(state) = app.try_state::<Arc<PendingDeepLink>>() {
            if let Ok(mut guard) = state.0.lock() {
                *guard = Some(payload.clone());
            }
        }
        let _ = app.emit("oppa://deep-link", payload);
    }
}

/// Parses an `<scheme>://pair?server=<base64url>&key=<code>` deep link.
fn parse_deep_link_pair(raw_url: &str) -> Option<DeepLinkPayload> {
    let parsed = raw_url.parse::<url::Url>().ok()?;
    if parsed.scheme() != oppa_product::DEEP_LINK_SCHEME {
        return None;
    }
    // In oppa-dev://pair?..., "pair" is the URL host (authority), not the path.
    if parsed.host_str() != Some("pair") {
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

#[cfg(test)]
mod tests {
    use base64::Engine as _;

    use super::parse_deep_link_pair;

    fn link(server: &str, key: &str) -> String {
        format!(
            "{}://pair?server={}&key={key}",
            oppa_product::DEEP_LINK_SCHEME,
            base64::prelude::BASE64_URL_SAFE_NO_PAD.encode(server)
        )
    }

    #[test]
    fn pairing_link_decodes_unpadded_server_url() {
        let payload =
            parse_deep_link_pair(&link("https://print.example.com", "ABCD-1234")).expect("link");
        assert_eq!(payload.server_url, "https://print.example.com");
        assert_eq!(payload.pair_key, "ABCD-1234");
    }

    #[test]
    fn foreign_schemes_and_paths_are_ignored() {
        assert!(parse_deep_link_pair("https://pair?server=aHR0cA&key=1").is_none());
        let wrong_host = link("https://print.example.com", "1").replace("://pair", "://other");
        assert!(parse_deep_link_pair(&wrong_host).is_none());
        let missing_key = format!("{}://pair?server=aHR0cA", oppa_product::DEEP_LINK_SCHEME);
        assert!(parse_deep_link_pair(&missing_key).is_none());
    }
}
