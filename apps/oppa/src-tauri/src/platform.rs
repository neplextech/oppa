//! Runtime capability reporting for the current host platform.
//!
//! The frontend renders controls only for capabilities reported here, so an
//! unsupported feature never appears available. Product configuration can
//! further disable a capability but can never enable one the platform lacks.

use oppa_product::ProductFeatures;
use serde::Serialize;

/// Operating-system family hosting the agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformKind {
    Macos,
    Windows,
    Linux,
    Android,
    Ios,
    Other,
}

impl PlatformKind {
    /// The platform this binary was compiled for.
    pub const fn current() -> Self {
        if cfg!(target_os = "android") {
            Self::Android
        } else if cfg!(target_os = "ios") {
            Self::Ios
        } else if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else {
            Self::Other
        }
    }
}

/// How the agent keeps working when its UI is not in front.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundExecution {
    /// Desktop: closing the window hides it while the tray agent continues.
    Tray,
    /// Android: a foreground service keeps the process and connection alive.
    ForegroundService,
    /// iOS: the connection is held only while the app is active; bounded
    /// background time may finish in-flight jobs after it leaves the screen.
    ForegroundOnly,
}

/// Update delivery mechanism compiled for this platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateChannel {
    /// Signed Tauri desktop updater.
    SignedDesktop,
    /// Direct-download APK verified against the installed signing key and
    /// installed only after the user approves it.
    ApkDownload,
    /// Updates arrive through a store or are not offered.
    None,
}

/// Host capabilities exposed to the frontend and enforced by commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_excessive_bools)]
pub struct PlatformCapabilities {
    pub kind: PlatformKind,
    pub mobile: bool,
    /// Custom title bar, window controls, and window menu.
    pub window_controls: bool,
    pub system_tray: bool,
    pub start_on_login: bool,
    /// Operating-system print queues (CUPS or the Windows spooler).
    pub system_print_queues: bool,
    /// DNS-SD discovery of network printers through platform APIs.
    pub network_service_discovery: bool,
    pub background_execution: BackgroundExecution,
    /// Optional screen keep-awake while the agent is running.
    pub keep_awake: bool,
    pub updates: UpdateChannel,
    /// Writing a diagnostics file to a user-visible location.
    pub diagnostics_file_export: bool,
    /// An explicit "quit and stop agent" action.
    pub quit: bool,
    /// USB and Bluetooth printing (outside the initial mobile scope).
    pub usb_printing: bool,
}

impl PlatformCapabilities {
    /// Capabilities of the platform this binary was compiled for.
    pub const fn current() -> Self {
        Self::for_platform(PlatformKind::current())
    }

    /// Capability table for one platform. Pure so every platform's mapping
    /// can be regression-tested on any host.
    pub const fn for_platform(kind: PlatformKind) -> Self {
        match kind {
            PlatformKind::Android => Self {
                kind,
                mobile: true,
                window_controls: false,
                system_tray: false,
                start_on_login: false,
                system_print_queues: false,
                network_service_discovery: true,
                background_execution: BackgroundExecution::ForegroundService,
                keep_awake: false,
                updates: UpdateChannel::ApkDownload,
                diagnostics_file_export: false,
                quit: true,
                usb_printing: false,
            },
            PlatformKind::Ios => Self {
                kind,
                mobile: true,
                window_controls: false,
                system_tray: false,
                start_on_login: false,
                system_print_queues: false,
                network_service_discovery: true,
                background_execution: BackgroundExecution::ForegroundOnly,
                keep_awake: true,
                updates: UpdateChannel::None,
                diagnostics_file_export: false,
                // Apple's guidelines forbid programmatic app termination.
                quit: false,
                usb_printing: false,
            },
            PlatformKind::Macos | PlatformKind::Windows | PlatformKind::Linux => Self {
                kind,
                mobile: false,
                window_controls: true,
                system_tray: true,
                start_on_login: true,
                system_print_queues: true,
                network_service_discovery: false,
                background_execution: BackgroundExecution::Tray,
                keep_awake: false,
                updates: UpdateChannel::SignedDesktop,
                diagnostics_file_export: true,
                quit: true,
                usb_printing: false,
            },
            PlatformKind::Other => Self {
                kind,
                mobile: false,
                window_controls: true,
                system_tray: true,
                start_on_login: false,
                system_print_queues: false,
                network_service_discovery: false,
                background_execution: BackgroundExecution::Tray,
                keep_awake: false,
                updates: UpdateChannel::None,
                diagnostics_file_export: true,
                quit: true,
                usb_printing: false,
            },
        }
    }

    /// Intersects compile-time product features with platform support.
    ///
    /// A product may disable any feature, but a feature the platform lacks
    /// stays disabled regardless of the product file.
    pub const fn effective_features(self, product: ProductFeatures) -> ProductFeatures {
        ProductFeatures {
            virtual_printer: product.virtual_printer,
            system_printer_discovery: product.system_printer_discovery && self.system_print_queues,
            network_printer_discovery: product.network_printer_discovery,
            usb_printer_discovery: product.usb_printer_discovery && self.usb_printing,
            remote_diagnostics: product.remote_diagnostics,
            managed_hardware: product.managed_hardware,
        }
    }
}

/// User-facing operating-system label for diagnostics.
pub fn display_operating_system(value: &str) -> &str {
    match value {
        "macos" => "macOS",
        "windows" => "Windows",
        "linux" => "Linux",
        "android" => "Android",
        "ios" => "iOS",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_features() -> ProductFeatures {
        ProductFeatures {
            virtual_printer: true,
            system_printer_discovery: true,
            network_printer_discovery: true,
            usb_printer_discovery: true,
            remote_diagnostics: true,
            managed_hardware: true,
        }
    }

    #[test]
    fn mobile_platforms_never_report_desktop_only_capabilities() {
        for kind in [PlatformKind::Android, PlatformKind::Ios] {
            let capabilities = PlatformCapabilities::for_platform(kind);
            assert!(capabilities.mobile);
            assert!(!capabilities.window_controls);
            assert!(!capabilities.system_tray);
            assert!(!capabilities.start_on_login);
            assert!(!capabilities.system_print_queues);
            assert!(!capabilities.diagnostics_file_export);
            assert_ne!(capabilities.updates, UpdateChannel::SignedDesktop);
            assert!(capabilities.network_service_discovery);
            assert!(!capabilities.usb_printing);
        }
    }

    #[test]
    fn lifecycle_models_match_platform_rules() {
        assert_eq!(
            PlatformCapabilities::for_platform(PlatformKind::Android).background_execution,
            BackgroundExecution::ForegroundService
        );
        let ios = PlatformCapabilities::for_platform(PlatformKind::Ios);
        assert_eq!(
            ios.background_execution,
            BackgroundExecution::ForegroundOnly
        );
        assert!(ios.keep_awake);
        assert!(!ios.quit, "iOS apps must not terminate themselves");
        assert_eq!(ios.updates, UpdateChannel::None);
        assert_eq!(
            PlatformCapabilities::for_platform(PlatformKind::Android).updates,
            UpdateChannel::ApkDownload
        );
    }

    #[test]
    fn desktop_capabilities_are_preserved() {
        for kind in [
            PlatformKind::Macos,
            PlatformKind::Windows,
            PlatformKind::Linux,
        ] {
            let capabilities = PlatformCapabilities::for_platform(kind);
            assert!(capabilities.window_controls);
            assert!(capabilities.system_tray);
            assert!(capabilities.start_on_login);
            assert!(capabilities.system_print_queues);
            assert_eq!(capabilities.background_execution, BackgroundExecution::Tray);
            assert_eq!(capabilities.updates, UpdateChannel::SignedDesktop);
        }
    }

    #[test]
    fn product_cannot_enable_missing_platform_capabilities() {
        let android = PlatformCapabilities::for_platform(PlatformKind::Android);
        let effective = android.effective_features(all_features());
        assert!(!effective.system_printer_discovery);
        assert!(!effective.usb_printer_discovery);
        assert!(effective.network_printer_discovery);
        assert!(effective.virtual_printer);

        let disabled = ProductFeatures {
            network_printer_discovery: false,
            ..all_features()
        };
        assert!(
            !android
                .effective_features(disabled)
                .network_printer_discovery
        );
    }

    #[test]
    fn compiled_platform_matches_target() {
        let current = PlatformCapabilities::current();
        assert_eq!(current.mobile, cfg!(mobile));
    }

    #[test]
    fn platform_labels_are_user_facing() {
        assert_eq!(display_operating_system("macos"), "macOS");
        assert_eq!(display_operating_system("windows"), "Windows");
        assert_eq!(display_operating_system("ios"), "iOS");
        assert_eq!(display_operating_system("android"), "Android");
    }
}
