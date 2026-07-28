use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};

use futures_util::future::join_all;
use oppa_printer::{
    DiscoveredPrinter, PrinterAvailability, PrinterCapabilities, PrinterConnection,
    PrinterValidationError, ProviderMetadata,
};
use tokio::sync::Mutex;

use crate::DiscoveryProvider;

/// One provider failure captured without aborting the inventory pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderFailure {
    /// Provider that failed.
    pub provider: String,
    /// Sanitized error safe for local diagnostics.
    pub message: String,
}

/// Difference from the previous successful discovery pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InventoryChange {
    /// A previously unseen printer observation appeared.
    Added(DiscoveredPrinter),
    /// Details for a still-visible printer changed.
    Updated {
        /// Previous normalized observation.
        previous: DiscoveredPrinter,
        /// Current normalized observation.
        current: Box<DiscoveredPrinter>,
    },
    /// A previously visible printer was no longer observed.
    Removed(DiscoveredPrinter),
}

/// Result of one fault-isolated discovery pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoverySnapshot {
    /// Normalized, deduplicated printer observations.
    pub printers: Vec<DiscoveredPrinter>,
    /// Differences from the manager's prior pass.
    pub changes: Vec<InventoryChange>,
    /// Providers that timed out or failed.
    pub failures: Vec<ProviderFailure>,
}

/// Executes and reconciles enabled discovery providers.
pub struct DiscoveryManager {
    providers: Vec<Arc<dyn DiscoveryProvider>>,
    provider_timeout: Duration,
    previous: Mutex<BTreeMap<String, DiscoveredPrinter>>,
}

impl DiscoveryManager {
    /// Creates a manager with an explicit deadline applied independently to
    /// every provider.
    #[must_use]
    pub fn new(providers: Vec<Arc<dyn DiscoveryProvider>>, provider_timeout: Duration) -> Self {
        Self {
            providers,
            provider_timeout,
            previous: Mutex::new(BTreeMap::new()),
        }
    }

    /// Runs all providers concurrently and emits a reconciled snapshot.
    pub async fn discover(&self) -> DiscoverySnapshot {
        let calls = self.providers.iter().map(|provider| {
            let provider = Arc::clone(provider);
            let deadline = self.provider_timeout;
            async move {
                let name = provider.name().to_owned();
                let result = tokio::time::timeout(deadline, provider.discover()).await;
                (name, result)
            }
        });

        let mut observations = Vec::new();
        let mut failures = Vec::new();
        for (provider, result) in join_all(calls).await {
            match result {
                Ok(Ok(printers)) => observations.extend(printers),
                Ok(Err(error)) => failures.push(ProviderFailure {
                    provider,
                    message: error.to_string(),
                }),
                Err(_) => failures.push(ProviderFailure {
                    provider,
                    message: format!("provider timed out after {:?}", self.provider_timeout),
                }),
            }
        }

        let mut valid = Vec::new();
        for printer in observations {
            match normalize_printer(printer) {
                Ok(printer) => valid.push(printer),
                Err(error) => failures.push(ProviderFailure {
                    provider: "normalization".to_owned(),
                    message: error.to_string(),
                }),
            }
        }
        let printers = deduplicate(valid);
        let current = printers
            .iter()
            .cloned()
            .map(|printer| (inventory_key(&printer), printer))
            .collect::<BTreeMap<_, _>>();

        let mut previous = self.previous.lock().await;
        let changes = inventory_changes(&previous, &current);
        *previous = current;

        DiscoverySnapshot {
            printers,
            changes,
            failures,
        }
    }
}

/// Normalizes provider-controlled strings and validates one observation.
pub fn normalize_printer(
    mut printer: DiscoveredPrinter,
) -> Result<DiscoveredPrinter, PrinterValidationError> {
    printer.name = printer.name.trim().to_owned();
    match &mut printer.connection {
        PrinterConnection::SystemQueue { queue_name } => {
            *queue_name = queue_name.trim().to_owned();
        }
        PrinterConnection::Network { host, .. } => {
            *host = host.trim().trim_end_matches('.').to_ascii_lowercase();
        }
        PrinterConnection::Usb { serial_number, .. } => {
            if let Some(serial) = serial_number {
                *serial = serial.trim().to_owned();
                if serial.is_empty() {
                    *serial_number = None;
                }
            }
        }
        PrinterConnection::Virtual { printer_id } => {
            *printer_id = printer_id.trim().to_owned();
        }
    }
    for value in [
        &mut printer.fingerprint.queue_name,
        &mut printer.fingerprint.driver_name,
        &mut printer.fingerprint.device_uri,
        &mut printer.fingerprint.host,
        &mut printer.fingerprint.serial_number,
        &mut printer.fingerprint.mac_address,
    ] {
        if let Some(inner) = value {
            *inner = inner.trim().to_owned();
            if inner.is_empty() {
                *value = None;
            }
        }
    }
    if let Some(host) = &mut printer.fingerprint.host {
        *host = host.trim_end_matches('.').to_ascii_lowercase();
    }
    printer.providers.sort_by(|left, right| {
        (&left.provider, &left.provider_id).cmp(&(&right.provider, &right.provider_id))
    });
    printer.providers.dedup_by(|left, right| {
        left.provider == right.provider && left.provider_id == right.provider_id
    });
    printer.validate()?;
    Ok(printer)
}

/// Merges observations that share at least one normalized identity key.
///
/// Provider metadata is retained. Conflicting observations remain separate
/// unless there is positive fingerprint evidence connecting them.
#[must_use]
pub fn deduplicate(printers: Vec<DiscoveredPrinter>) -> Vec<DiscoveredPrinter> {
    let mut merged: Vec<DiscoveredPrinter> = Vec::new();
    for printer in printers {
        let keys = identity_keys(&printer);
        let match_index = merged.iter().position(|existing| {
            let existing_keys = identity_keys(existing);
            !keys.is_disjoint(&existing_keys)
        });
        if let Some(index) = match_index {
            merge_observation(&mut merged[index], printer);
        } else {
            merged.push(printer);
        }
    }
    merged.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| inventory_key(left).cmp(&inventory_key(right)))
    });
    merged
}

fn identity_keys(printer: &DiscoveredPrinter) -> BTreeSet<String> {
    let mut keys = printer
        .fingerprint
        .identity_keys()
        .into_iter()
        .collect::<BTreeSet<_>>();
    match &printer.connection {
        PrinterConnection::SystemQueue { queue_name } => {
            keys.insert(format!("queue:{}", queue_name.trim().to_lowercase()));
        }
        PrinterConnection::Network { host, port } => {
            keys.insert(format!(
                "network:{}:{port}",
                host.trim_end_matches('.').to_lowercase()
            ));
        }
        PrinterConnection::Usb {
            vendor_id,
            product_id,
            serial_number: Some(serial),
        } => {
            keys.insert(format!(
                "usb:{vendor_id:04x}:{product_id:04x}:{}",
                serial.trim().to_lowercase()
            ));
        }
        PrinterConnection::Virtual { printer_id } => {
            keys.insert(format!("virtual:{}", printer_id.to_lowercase()));
        }
        PrinterConnection::Usb {
            serial_number: None,
            ..
        } => {}
    }
    keys
}

fn merge_observation(destination: &mut DiscoveredPrinter, mut source: DiscoveredPrinter) {
    destination.fingerprint.merge_missing(&source.fingerprint);
    if destination.id.is_none() {
        destination.id = source.id.take();
    }
    if destination.capabilities.is_none() {
        destination.capabilities = source.capabilities.take();
    } else if let (Some(existing), Some(additional)) =
        (&mut destination.capabilities, source.capabilities.take())
    {
        merge_capabilities(existing, &additional);
    }
    if availability_rank(source.availability) > availability_rank(destination.availability) {
        destination.availability = source.availability;
    }
    for provider in source.providers {
        if !destination.providers.iter().any(|existing| {
            existing.provider == provider.provider && existing.provider_id == provider.provider_id
        }) {
            destination.providers.push(provider);
        }
    }
    destination.providers.sort_by(|left, right| {
        (&left.provider, &left.provider_id).cmp(&(&right.provider, &right.provider_id))
    });
}

fn availability_rank(availability: PrinterAvailability) -> u8 {
    match availability {
        PrinterAvailability::Online => 3,
        PrinterAvailability::Degraded => 2,
        PrinterAvailability::Unknown => 1,
        PrinterAvailability::Offline => 0,
    }
}

fn merge_capabilities(destination: &mut PrinterCapabilities, source: &PrinterCapabilities) {
    destination
        .receipt_widths_mm
        .extend(source.receipt_widths_mm.iter().copied());
    destination.receipt_widths_mm.sort_unstable();
    destination.receipt_widths_mm.dedup();
    destination.esc_pos |= source.esc_pos;
    destination.raster |= source.raster;
    destination.cut |= source.cut;
    destination.qr_code |= source.qr_code;
    destination.barcode |= source.barcode;
    destination.cancellation |= source.cancellation;
}

fn inventory_key(printer: &DiscoveredPrinter) -> String {
    if let Some(id) = &printer.id {
        return format!("id:{id}");
    }
    identity_keys(printer)
        .into_iter()
        .next()
        .unwrap_or_else(|| {
            let provider = printer
                .providers
                .first()
                .cloned()
                .unwrap_or(ProviderMetadata {
                    provider: "unknown".to_owned(),
                    provider_id: None,
                    attributes: BTreeMap::new(),
                });
            format!(
                "provider:{}:{}:{}",
                provider.provider,
                provider.provider_id.unwrap_or_default(),
                printer.name.to_lowercase()
            )
        })
}

fn inventory_changes(
    previous: &BTreeMap<String, DiscoveredPrinter>,
    current: &BTreeMap<String, DiscoveredPrinter>,
) -> Vec<InventoryChange> {
    let mut changes = Vec::new();
    for (key, printer) in current {
        match previous.get(key) {
            None => changes.push(InventoryChange::Added(printer.clone())),
            Some(old) if old != printer => changes.push(InventoryChange::Updated {
                previous: old.clone(),
                current: Box::new(printer.clone()),
            }),
            Some(_) => {}
        }
    }
    for (key, printer) in previous {
        if !current.contains_key(key) {
            changes.push(InventoryChange::Removed(printer.clone()));
        }
    }
    changes
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use async_trait::async_trait;
    use oppa_printer::{PrinterConnection, PrinterFingerprint, PrinterKind, ProviderMetadata};

    use super::*;
    use crate::{DiscoveryError, DiscoveryProvider, DiscoveryResult};

    fn printer(provider: &str, queue: &str, host: Option<&str>) -> DiscoveredPrinter {
        DiscoveredPrinter {
            id: None,
            name: format!(" {queue} "),
            kind: PrinterKind::Receipt,
            connection: PrinterConnection::SystemQueue {
                queue_name: format!(" {queue} "),
            },
            fingerprint: PrinterFingerprint {
                queue_name: Some(queue.to_owned()),
                host: host.map(str::to_owned),
                port: host.map(|_| 9100),
                ..PrinterFingerprint::default()
            },
            availability: PrinterAvailability::Unknown,
            capabilities: None,
            providers: vec![ProviderMetadata {
                provider: provider.to_owned(),
                provider_id: Some(queue.to_owned()),
                attributes: BTreeMap::new(),
            }],
        }
    }

    struct Provider {
        name: &'static str,
        result: Result<Vec<DiscoveredPrinter>, &'static str>,
    }

    #[async_trait]
    impl DiscoveryProvider for Provider {
        fn name(&self) -> &'static str {
            self.name
        }

        async fn discover(&self) -> DiscoveryResult<Vec<DiscoveredPrinter>> {
            self.result
                .clone()
                .map_err(|message| DiscoveryError::Unavailable(message.to_owned()))
        }
    }

    #[test]
    fn likely_duplicates_merge_and_preserve_providers() {
        let first = normalize_printer(printer("system", "Receipt", Some("printer.local")))
            .expect("valid first");
        let second = normalize_printer(printer("manual", "Other", Some("PRINTER.local.")))
            .expect("valid second");
        let merged = deduplicate(vec![first, second]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].providers.len(), 2);
    }

    #[tokio::test]
    async fn provider_failure_does_not_hide_healthy_inventory() {
        let manager = DiscoveryManager::new(
            vec![
                Arc::new(Provider {
                    name: "healthy",
                    result: Ok(vec![printer("healthy", "Receipt", None)]),
                }),
                Arc::new(Provider {
                    name: "missing",
                    result: Err("lpstat not installed"),
                }),
            ],
            Duration::from_secs(1),
        );
        let first = manager.discover().await;
        assert_eq!(first.printers.len(), 1);
        assert_eq!(first.failures.len(), 1);
        assert!(matches!(
            first.changes.as_slice(),
            [InventoryChange::Added(_)]
        ));

        let second = manager.discover().await;
        assert!(second.changes.is_empty());
    }
}
