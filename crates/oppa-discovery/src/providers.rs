use std::{collections::BTreeMap, process::Stdio, sync::Arc, time::Duration};

use async_trait::async_trait;
use oppa_core::PrinterId;
use oppa_printer::{
    DiscoveredPrinter, PrinterAvailability, PrinterCapabilities, PrinterConnection,
    PrinterFingerprint, PrinterKind, ProviderMetadata,
};
use tokio::{process::Command, sync::RwLock};

use crate::{DiscoveryError, DiscoveryProvider, DiscoveryResult};

/// One manually configured raw TCP printer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualNetworkPrinter {
    /// Stable identity assigned by local configuration.
    pub id: PrinterId,
    /// User-facing local name.
    pub name: String,
    /// Hostname or IP address.
    pub host: String,
    /// TCP port, commonly 9100.
    pub port: u16,
}

/// Provider backed by explicit local network-printer configuration.
#[derive(Debug, Clone, Default)]
pub struct ManualNetworkProvider {
    printers: Vec<ManualNetworkPrinter>,
}

impl ManualNetworkProvider {
    /// Creates a provider from a settings snapshot.
    #[must_use]
    pub fn new(printers: Vec<ManualNetworkPrinter>) -> Self {
        Self { printers }
    }
}

#[async_trait]
impl DiscoveryProvider for ManualNetworkProvider {
    fn name(&self) -> &'static str {
        "manual-network"
    }

    async fn discover(&self) -> DiscoveryResult<Vec<DiscoveredPrinter>> {
        self.printers
            .iter()
            .map(|printer| {
                let observation = DiscoveredPrinter {
                    id: Some(printer.id.clone()),
                    name: printer.name.clone(),
                    kind: PrinterKind::Receipt,
                    connection: PrinterConnection::Network {
                        host: printer.host.clone(),
                        port: printer.port,
                    },
                    fingerprint: PrinterFingerprint {
                        host: Some(printer.host.clone()),
                        port: Some(printer.port),
                        ..PrinterFingerprint::default()
                    },
                    availability: PrinterAvailability::Unknown,
                    capabilities: Some(receipt_capabilities()),
                    providers: vec![ProviderMetadata {
                        provider: self.name().to_owned(),
                        provider_id: Some(printer.id.to_string()),
                        attributes: BTreeMap::new(),
                    }],
                };
                observation
                    .validate()
                    .map_err(|error| DiscoveryError::InvalidData(error.to_string()))?;
                Ok(observation)
            })
            .collect()
    }
}

/// Configured virtual printer exposed through discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualPrinterDefinition {
    /// Stable OPPA identity.
    pub id: PrinterId,
    /// User-facing local name.
    pub name: String,
    /// Simulated current availability.
    pub availability: PrinterAvailability,
}

/// Mutable in-process virtual-printer provider.
#[derive(Debug, Clone, Default)]
pub struct VirtualPrinterProvider {
    printers: Arc<RwLock<Vec<VirtualPrinterDefinition>>>,
}

impl VirtualPrinterProvider {
    /// Creates a provider with initial definitions.
    #[must_use]
    pub fn new(printers: Vec<VirtualPrinterDefinition>) -> Self {
        Self {
            printers: Arc::new(RwLock::new(printers)),
        }
    }

    /// Atomically replaces the configured virtual-printer snapshot.
    pub async fn replace(&self, printers: Vec<VirtualPrinterDefinition>) {
        *self.printers.write().await = printers;
    }
}

#[async_trait]
impl DiscoveryProvider for VirtualPrinterProvider {
    fn name(&self) -> &'static str {
        "virtual"
    }

    async fn discover(&self) -> DiscoveryResult<Vec<DiscoveredPrinter>> {
        let printers = self.printers.read().await;
        Ok(printers
            .iter()
            .map(|printer| DiscoveredPrinter {
                id: Some(printer.id.clone()),
                name: printer.name.clone(),
                kind: PrinterKind::Virtual,
                connection: PrinterConnection::Virtual {
                    printer_id: printer.id.to_string(),
                },
                fingerprint: PrinterFingerprint::default(),
                availability: printer.availability,
                capabilities: Some(receipt_capabilities()),
                providers: vec![ProviderMetadata {
                    provider: self.name().to_owned(),
                    provider_id: Some(printer.id.to_string()),
                    attributes: BTreeMap::new(),
                }],
            })
            .collect())
    }
}

fn receipt_capabilities() -> PrinterCapabilities {
    PrinterCapabilities {
        receipt_widths_mm: vec![58, 80],
        esc_pos: true,
        raster: true,
        cut: true,
        qr_code: true,
        barcode: true,
        cancellation: true,
    }
}

/// Discovers operating-system printer queues using a bounded platform command.
#[derive(Debug, Clone)]
pub struct SystemQueueProvider {
    command_timeout: Duration,
}

impl SystemQueueProvider {
    /// Creates a provider with the given platform-command deadline.
    #[must_use]
    pub const fn new(command_timeout: Duration) -> Self {
        Self { command_timeout }
    }
}

impl Default for SystemQueueProvider {
    fn default() -> Self {
        Self::new(Duration::from_secs(5))
    }
}

#[async_trait]
impl DiscoveryProvider for SystemQueueProvider {
    fn name(&self) -> &'static str {
        "system-queue"
    }

    async fn discover(&self) -> DiscoveryResult<Vec<DiscoveredPrinter>> {
        let mut command = platform_queue_command();
        command
            .stdin(Stdio::null())
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
            .kill_on_drop(true);
        let output = tokio::time::timeout(self.command_timeout, command.output())
            .await
            .map_err(|_| DiscoveryError::Timeout(self.command_timeout))?
            .map_err(|error| {
                DiscoveryError::Unavailable(format!("cannot execute printer query: {error}"))
            })?;
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(DiscoveryError::Unavailable(format!(
                "printer query exited with {}: {}",
                output.status,
                error.trim()
            )));
        }
        let output = String::from_utf8(output.stdout).map_err(|error| {
            DiscoveryError::InvalidData(format!("printer query returned non-UTF-8 output: {error}"))
        })?;
        parse_platform_queues(&output)
    }
}

#[cfg(unix)]
fn platform_queue_command() -> Command {
    let mut command = Command::new("lpstat");
    command.args(["-p", "-v"]);
    command
}

#[cfg(windows)]
fn platform_queue_command() -> Command {
    let mut command = Command::new("powershell.exe");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "Get-Printer | ForEach-Object { \"$($_.Name)`t$($_.DriverName)`t$($_.PortName)\" }",
    ]);
    command
}

#[cfg(not(any(unix, windows)))]
fn platform_queue_command() -> Command {
    Command::new("__oppa_unsupported_printer_query__")
}

#[cfg(unix)]
fn parse_platform_queues(output: &str) -> DiscoveryResult<Vec<DiscoveredPrinter>> {
    Ok(parse_lpstat(output))
}

#[cfg(windows)]
fn parse_platform_queues(output: &str) -> DiscoveryResult<Vec<DiscoveredPrinter>> {
    parse_windows_printers(output)
}

#[cfg(not(any(unix, windows)))]
fn parse_platform_queues(_output: &str) -> DiscoveryResult<Vec<DiscoveredPrinter>> {
    Err(DiscoveryError::Unavailable(
        "system printer discovery is unsupported on this platform".to_owned(),
    ))
}

/// Parses combined `lpstat -p -v` output.
///
/// Unknown lines are ignored because CUPS may localize status prose; queue and
/// device records are joined by the exact queue identifier.
#[must_use]
pub fn parse_lpstat(output: &str) -> Vec<DiscoveredPrinter> {
    #[derive(Default)]
    struct Queue {
        seen: bool,
        offline: bool,
        uri: Option<String>,
    }
    let mut queues: BTreeMap<String, Queue> = BTreeMap::new();
    for raw_line in output.lines() {
        let line = raw_line.trim();
        if let Some(rest) = line.strip_prefix("printer ")
            && let Some((name, status)) = rest.split_once(' ')
        {
            let queue = queues.entry(name.to_owned()).or_default();
            queue.seen = true;
            let status = status.to_ascii_lowercase();
            queue.offline = status.contains("disabled") || status.contains("offline");
        } else if let Some(rest) = line.strip_prefix("device for ")
            && let Some((name, uri)) = rest.split_once(':')
        {
            let uri = uri.trim();
            if !name.trim().is_empty() && !uri.is_empty() {
                queues.entry(name.trim().to_owned()).or_default().uri = Some(uri.to_owned());
            }
        }
    }
    queues
        .into_iter()
        .filter(|(_, queue)| queue.seen || queue.uri.is_some())
        .map(|(name, queue)| {
            let mut attributes = BTreeMap::new();
            if let Some(uri) = &queue.uri {
                attributes.insert("deviceUri".to_owned(), uri.clone());
            }
            DiscoveredPrinter {
                id: None,
                name: name.clone(),
                kind: PrinterKind::Unknown,
                connection: PrinterConnection::SystemQueue {
                    queue_name: name.clone(),
                },
                fingerprint: PrinterFingerprint {
                    queue_name: Some(name.clone()),
                    device_uri: queue.uri,
                    ..PrinterFingerprint::default()
                },
                availability: if queue.offline {
                    PrinterAvailability::Offline
                } else {
                    PrinterAvailability::Unknown
                },
                capabilities: None,
                providers: vec![ProviderMetadata {
                    provider: "system-queue".to_owned(),
                    provider_id: Some(name),
                    attributes,
                }],
            }
        })
        .collect()
}

#[cfg(windows)]
fn parse_windows_printers(output: &str) -> DiscoveryResult<Vec<DiscoveredPrinter>> {
    let mut printers = Vec::new();
    for (index, line) in output.lines().enumerate() {
        let fields = line.split('\t').map(str::trim).collect::<Vec<_>>();
        if fields.len() != 3 || fields[0].is_empty() {
            return Err(DiscoveryError::InvalidData(format!(
                "malformed Windows printer record on line {}",
                index + 1
            )));
        }
        let mut attributes = BTreeMap::new();
        attributes.insert("driverName".to_owned(), fields[1].to_owned());
        attributes.insert("portName".to_owned(), fields[2].to_owned());
        printers.push(DiscoveredPrinter {
            id: None,
            name: fields[0].to_owned(),
            kind: PrinterKind::Unknown,
            connection: PrinterConnection::SystemQueue {
                queue_name: fields[0].to_owned(),
            },
            fingerprint: PrinterFingerprint {
                queue_name: Some(fields[0].to_owned()),
                driver_name: Some(fields[1].to_owned()),
                device_uri: Some(fields[2].to_owned()),
                ..PrinterFingerprint::default()
            },
            availability: PrinterAvailability::Unknown,
            capabilities: None,
            providers: vec![ProviderMetadata {
                provider: "system-queue".to_owned(),
                provider_id: Some(fields[0].to_owned()),
                attributes,
            }],
        });
    }
    Ok(printers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lpstat_parser_joins_queue_and_device_lines() {
        let printers = parse_lpstat(
            "printer Receipt is idle. enabled since today\n\
             printer Offline disabled since yesterday\n\
             device for Receipt: socket://printer.local:9100\n\
             device for Offline: usb://example/model\n",
        );
        assert_eq!(printers.len(), 2);
        let receipt = printers
            .iter()
            .find(|printer| printer.name == "Receipt")
            .expect("receipt queue");
        assert_eq!(
            receipt.fingerprint.device_uri.as_deref(),
            Some("socket://printer.local:9100")
        );
        let offline = printers
            .iter()
            .find(|printer| printer.name == "Offline")
            .expect("offline queue");
        assert_eq!(offline.availability, PrinterAvailability::Offline);
    }
}
