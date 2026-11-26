//! Persisted mobile lifecycle preferences and user-facing lifecycle status.
//!
//! Compiled on every platform so the defaults, limitation text, and vendor
//! battery guidance are regression-tested without a device.

// Used by the mobile host; desktop builds compile it for regression tests.
#![cfg_attr(desktop, allow(dead_code))]

use oppa_storage::SqliteStorage;
use serde::{Deserialize, Serialize};

use crate::{
    error::CommandError,
    platform::{BackgroundExecution, PlatformCapabilities, PlatformKind},
};

/// `SQLite` setting key for [`MobileSettings`].
pub const MOBILE_SETTINGS: &str = "mobile.lifecycle.v1";

/// User preferences for mobile background operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MobileSettings {
    /// Android: run the persistent foreground service while paired.
    pub background_agent: bool,
    /// iOS: keep the screen awake while the agent is paired and active.
    pub keep_awake: bool,
}

impl Default for MobileSettings {
    fn default() -> Self {
        Self {
            background_agent: true,
            keep_awake: false,
        }
    }
}

impl MobileSettings {
    /// Loads settings, falling back to defaults for absent or invalid data.
    pub async fn load(storage: &SqliteStorage) -> Self {
        storage
            .setting(MOBILE_SETTINGS)
            .await
            .ok()
            .flatten()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default()
    }

    /// Persists settings.
    pub async fn save(self, storage: &SqliteStorage) -> Result<(), CommandError> {
        let value = serde_json::to_value(self)
            .map_err(|error| CommandError::internal(error.to_string()))?;
        storage
            .set_setting(MOBILE_SETTINGS, &value)
            .await
            .map_err(|error| CommandError::internal(error.to_string()))
    }
}

/// Vendor-specific steps for battery managers that stop background apps
/// despite a foreground service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatteryGuidance {
    pub vendor: String,
    pub steps: Vec<String>,
    /// Independent reference describing the vendor's restrictions.
    pub reference_url: String,
}

/// Lifecycle state rendered by the mobile Settings and Overview screens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MobileLifecycleStatus {
    pub platform: PlatformKind,
    pub background_execution: BackgroundExecution,
    pub settings: MobileSettings,
    /// Android: the foreground service is running.
    pub background_service_running: bool,
    /// Android: the persistent notification can be shown.
    pub notifications_granted: Option<bool>,
    /// Android: the app is exempt from battery optimization.
    pub ignoring_battery_optimizations: Option<bool>,
    /// iOS: local network privacy decision (`granted`, `denied`, `undetermined`).
    pub local_network_access: Option<String>,
    pub battery_guidance: Option<BatteryGuidance>,
    /// Platform limitations the user must know about.
    pub limitations: Vec<String>,
}

/// Fixed limitations for each lifecycle model.
pub fn limitations(capabilities: PlatformCapabilities) -> Vec<String> {
    match capabilities.background_execution {
        BackgroundExecution::ForegroundOnly => vec![
            "iOS does not allow continuous unattended background printing. Keep OPPA open on screen to receive print jobs.".to_owned(),
            "When OPPA leaves the screen, jobs already printing get a short, system-limited time to finish. New jobs wait on the server until OPPA is opened again.".to_owned(),
            "OPPA reconnects and resumes pending jobs automatically when it becomes active.".to_owned(),
        ],
        BackgroundExecution::ForegroundService => vec![
            "OPPA keeps printing while you use other apps or the screen is locked, and shows a notification while it runs.".to_owned(),
            "If Android closes OPPA to save memory, the notification asks you to reopen it; pending jobs resume after reconnecting.".to_owned(),
            "USB and Bluetooth printers are not supported yet; use a network printer.".to_owned(),
        ],
        BackgroundExecution::Tray => Vec::new(),
    }
}

/// Returns guidance for manufacturers known to stop foreground services.
#[cfg_attr(target_os = "ios", allow(dead_code))]
pub fn battery_guidance(manufacturer: &str) -> Option<BatteryGuidance> {
    let vendor = manufacturer.trim().to_ascii_lowercase();
    let (name, slug, steps): (&str, &str, &[&str]) = match vendor.as_str() {
        "xiaomi" | "redmi" | "poco" => (
            "Xiaomi",
            "xiaomi",
            &[
                "Settings → Apps → Manage apps → OPPA → Autostart: on.",
                "OPPA → Battery saver → No restrictions.",
                "Open Recents, long-press OPPA, and tap the lock icon.",
            ],
        ),
        "huawei" | "honor" => (
            "Huawei",
            "huawei",
            &[
                "Settings → Battery → App launch → OPPA: Manage manually.",
                "Enable Auto-launch, Secondary launch, and Run in background.",
            ],
        ),
        "samsung" => (
            "Samsung",
            "samsung",
            &[
                "Settings → Battery → Background usage limits → Never sleeping apps → add OPPA.",
                "Settings → Apps → OPPA → Battery → Unrestricted.",
            ],
        ),
        "oneplus" | "oppo" | "realme" => (
            "OnePlus / OPPO / realme",
            "oneplus",
            &[
                "Settings → Apps → OPPA → Battery usage → Allow background activity.",
                "Settings → Battery → More settings → Optimize battery use → OPPA: Don't optimize.",
            ],
        ),
        "vivo" | "iqoo" => (
            "vivo",
            "vivo",
            &[
                "Settings → Battery → Background power consumption management → OPPA: Allow.",
                "Settings → Apps → Autostart → OPPA: on.",
            ],
        ),
        "asus" => (
            "ASUS",
            "asus",
            &["Mobile Manager → PowerMaster → Auto-start manager → allow OPPA."],
        ),
        _ => return None,
    };
    Some(BatteryGuidance {
        vendor: name.to_owned(),
        steps: steps.iter().map(|step| (*step).to_owned()).collect(),
        reference_url: format!("https://dontkillmyapp.com/{slug}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_defaults_to_background_agent_and_ios_to_normal_sleep() {
        let settings = MobileSettings::default();
        assert!(settings.background_agent);
        assert!(!settings.keep_awake);
    }

    #[test]
    fn settings_tolerate_missing_fields() {
        let settings: MobileSettings = serde_json::from_str(r#"{"keepAwake":true}"#).expect("json");
        assert!(settings.keep_awake);
        assert!(settings.background_agent);
    }

    #[test]
    fn ios_limitations_state_that_unattended_background_printing_is_unsupported() {
        let ios = limitations(PlatformCapabilities::for_platform(PlatformKind::Ios));
        assert!(ios[0].contains("does not allow continuous unattended background printing"));
        assert_eq!(
            limitations(PlatformCapabilities::for_platform(PlatformKind::Macos)),
            Vec::<String>::new()
        );
        assert_ne!(
            limitations(PlatformCapabilities::for_platform(PlatformKind::Android)),
            Vec::<String>::new()
        );
    }

    #[test]
    fn known_vendor_battery_managers_get_actionable_guidance() {
        let guidance = battery_guidance(" Xiaomi ").expect("xiaomi");
        assert_eq!(guidance.reference_url, "https://dontkillmyapp.com/xiaomi");
        assert_ne!(guidance.steps, Vec::<String>::new());
        assert_eq!(battery_guidance("HONOR").expect("honor").vendor, "Huawei");
        assert!(battery_guidance("Google").is_none());
    }
}
