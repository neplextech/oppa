//! Serializable values exchanged with the Kotlin and Swift plugin classes.

// Response shapes are only deserialized by the mobile bridge.
#![cfg_attr(not(mobile), allow(dead_code))]

use serde::{Deserialize, Serialize};

/// Lifecycle, connectivity, and control notifications sent by the native
/// runtime to the Rust host.
///
/// Native code sends these through a Rust-owned Tauri channel, which does not
/// require the web view or an Android activity to be alive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum MobileEvent {
    /// An Android activity (and its web view) is available for plugin calls.
    ActivityAvailable,
    /// The last Android activity was destroyed; the process may continue in
    /// the foreground service.
    ActivityDestroyed,
    /// The visible UI returned to the foreground.
    Foregrounded,
    /// The visible UI moved to the background.
    Backgrounded,
    /// A usable default network became available or changed.
    NetworkAvailable,
    /// The default network was lost.
    NetworkLost,
    /// The user asked to stop the agent from the persistent notification.
    StopRequested,
    /// An iOS bounded background task reached its system deadline.
    BackgroundTaskExpired,
}

/// One printer-like network service found through platform service discovery.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceEndpoint {
    /// Advertised instance name.
    pub name: String,
    /// DNS-SD service type, for example `_pdl-datastream._tcp`.
    pub service_type: String,
    /// Resolved numeric address or host name.
    pub host: String,
    /// Resolved TCP port.
    pub port: u16,
}

/// Snapshot of Android background-execution prerequisites.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidBackgroundStatus {
    /// Whether the user enabled the persistent background agent.
    pub enabled: bool,
    /// Whether the foreground service is currently running.
    pub running: bool,
    /// Whether the persistent notification can be shown.
    pub notifications_granted: bool,
    /// Whether the app is exempt from battery optimizations (Doze).
    pub ignoring_battery_optimizations: bool,
    /// Device manufacturer, used to select vendor-specific guidance.
    pub manufacturer: String,
    /// Android API level.
    pub sdk_int: u32,
}

/// iOS local network privacy authorization as observed by a Bonjour browse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalNetworkAccess {
    /// The system has not reported a decision yet.
    Undetermined,
    /// Local network access is allowed.
    Granted,
    /// The user denied local network access in Settings.
    Denied,
}

#[cfg(target_os = "android")]
#[derive(Debug, Deserialize)]
pub(crate) struct CredentialValue {
    pub(crate) value: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ServiceEndpoints {
    pub(crate) services: Vec<ServiceEndpoint>,
}

#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackgroundTask {
    pub(crate) task_id: i64,
}

#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
#[derive(Debug, Deserialize)]
pub(crate) struct LocalNetworkState {
    pub(crate) access: LocalNetworkAccess,
}

#[cfg(target_os = "android")]
#[derive(Debug, Deserialize)]
pub(crate) struct BooleanValue {
    pub(crate) value: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_events_use_stable_discriminators() {
        let event: MobileEvent =
            serde_json::from_str(r#"{"type":"stopRequested"}"#).expect("event");
        assert_eq!(event, MobileEvent::StopRequested);
        let event: MobileEvent =
            serde_json::from_str(r#"{"type":"networkAvailable"}"#).expect("event");
        assert_eq!(event, MobileEvent::NetworkAvailable);
        assert!(serde_json::from_str::<MobileEvent>(r#"{"type":"printed"}"#).is_err());
    }

    #[test]
    fn service_endpoints_deserialize_from_native_shape() {
        let services: ServiceEndpoints = serde_json::from_str(
            r#"{"services":[{"name":"Kitchen","serviceType":"_pdl-datastream._tcp","host":"192.168.1.20","port":9100}]}"#,
        )
        .expect("services");
        assert_eq!(services.services[0].port, 9100);
    }
}
