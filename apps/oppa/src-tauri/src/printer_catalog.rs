use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use chrono::Utc;
use oppa_agent::StaticPrinterResolver;
use oppa_core::PrinterId;
use oppa_discovery::{
    DiscoveryManager, DiscoveryProvider, ManualNetworkPrinter, ManualNetworkProvider,
    SystemQueueProvider, VirtualPrinterDefinition, VirtualPrinterProvider,
};
use oppa_printer::{
    DiscoveredPrinter, PrinterAvailability, PrinterCapabilities as DomainPrinterCapabilities,
    PrinterConnection, PrinterFingerprint, PrinterRef, ProviderMetadata,
};
use oppa_product::ProductFeatures;
use oppa_protocol::{
    PrinterAvailability as ProtocolAvailability, PrinterCapabilities as ProtocolCapabilities,
    PrinterConnection as ProtocolConnection, PrinterDescriptor, PrinterKind as ProtocolPrinterKind,
    ReceiptWidth,
};
use oppa_renderer::RenderedDocument;
use oppa_storage::SqliteStorage;
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::{
    diagnostics::DiagnosticLog,
    error::CommandError,
    models::{
        CatalogPrinter, ConfigurePrinterChanges, DiscoveryProviderStatus, DocumentType,
        ManualPrinterInput, PersistedCatalog, PrinterCapabilities, PrinterConnectionType,
        PrinterSummary, VirtualOutput, VirtualOutputFormat, VirtualPrinterInput,
        VirtualPrinterMode,
    },
    virtual_spooler::PerPrinterVirtualSpooler,
};

const CATALOG_SETTING: &str = "desktop.printer-catalog.v1";
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_VIRTUAL_DELAY_MS: u64 = 60_000;
const MAX_ESC_POS_PREVIEW_BYTES: usize = 512;

/// Host-owned configured and discovered printer state.
pub struct PrinterCatalog {
    storage: SqliteStorage,
    state: RwLock<PersistedCatalog>,
    mutation: Mutex<()>,
    resolver: Arc<StaticPrinterResolver>,
    virtual_spooler: Arc<PerPrinterVirtualSpooler>,
    features: ProductFeatures,
    discovery_status: RwLock<Vec<DiscoveryProviderStatus>>,
    revision: AtomicU64,
    log: Arc<DiagnosticLog>,
}

impl PrinterCatalog {
    pub async fn load(
        storage: SqliteStorage,
        virtual_spooler: Arc<PerPrinterVirtualSpooler>,
        features: ProductFeatures,
        log: Arc<DiagnosticLog>,
    ) -> Result<Self, CommandError> {
        let persisted = match storage
            .setting(CATALOG_SETTING)
            .await
            .map_err(|error| CommandError::internal(error.to_string()))?
        {
            Some(value) => serde_json::from_value::<PersistedCatalog>(value).map_err(|error| {
                CommandError::internal(format!("printer catalog is invalid: {error}"))
            })?,
            None => PersistedCatalog::default(),
        };

        let mut seen = BTreeSet::new();
        for printer in &persisted.printers {
            printer.reference.validate().map_err(|error| {
                CommandError::internal(format!("stored printer is invalid: {error}"))
            })?;
            if !seen.insert(printer.reference.id.clone()) {
                return Err(CommandError::internal(format!(
                    "stored printer id {} is duplicated",
                    printer.reference.id
                )));
            }
            if printer.is_virtual() && feature_allows_printer(features, printer) {
                virtual_spooler
                    .register(
                        printer.reference.id.clone(),
                        persisted_virtual_mode(printer.virtual_mode),
                        printer.virtual_delay_ms,
                    )
                    .await;
            }
        }
        let resolver = Arc::new(
            StaticPrinterResolver::new(
                persisted
                    .printers
                    .iter()
                    .filter(|printer| feature_allows_printer(features, printer))
                    .map(|printer| printer.reference.clone()),
            )
            .map_err(|error| CommandError::internal(error.to_string()))?,
        );

        Ok(Self {
            storage,
            state: RwLock::new(persisted),
            mutation: Mutex::new(()),
            resolver,
            virtual_spooler,
            features,
            discovery_status: RwLock::new(Vec::new()),
            revision: AtomicU64::new(1),
            log,
        })
    }

    pub fn resolver(&self) -> Arc<StaticPrinterResolver> {
        Arc::clone(&self.resolver)
    }

    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::Relaxed)
    }

    pub fn bump_revision(&self) -> u64 {
        self.revision
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                Some(current.saturating_add(1))
            })
            .unwrap_or_else(|current| current)
            .saturating_add(1)
    }

    pub async fn discovery_status(&self) -> Vec<DiscoveryProviderStatus> {
        self.discovery_status.read().await.clone()
    }

    pub async fn list(&self) -> Result<Vec<PrinterSummary>, CommandError> {
        let printers = self
            .state
            .read()
            .await
            .printers
            .iter()
            .filter(|printer| self.is_active(printer))
            .cloned()
            .collect::<Vec<_>>();
        let mut summaries = Vec::with_capacity(printers.len());
        for printer in printers {
            summaries.push(self.summary(&printer).await?);
        }
        summaries.sort_by(|left, right| {
            left.display_name
                .to_lowercase()
                .cmp(&right.display_name.to_lowercase())
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(summaries)
    }

    #[allow(clippy::too_many_lines)]
    pub async fn refresh(&self) -> Result<Vec<PrinterSummary>, CommandError> {
        let snapshot = self.state.read().await.clone();
        let mut providers: Vec<Arc<dyn DiscoveryProvider>> = Vec::new();
        if self.features.system_printer_discovery {
            providers.push(Arc::new(SystemQueueProvider::default()));
        }
        if self.features.network_printer_discovery {
            let manual = snapshot
                .printers
                .iter()
                .filter_map(|printer| {
                    let PrinterConnection::Network { host, port } = &printer.reference.connection
                    else {
                        return None;
                    };
                    Some(ManualNetworkPrinter {
                        id: printer.reference.id.clone(),
                        name: printer.reference.display_name.clone(),
                        host: host.clone(),
                        port: *port,
                    })
                })
                .collect();
            providers.push(Arc::new(ManualNetworkProvider::new(manual)));
        }
        if self.features.virtual_printer {
            let virtual_printers = snapshot
                .printers
                .iter()
                .filter(|printer| printer.is_virtual())
                .map(|printer| VirtualPrinterDefinition {
                    id: printer.reference.id.clone(),
                    name: printer.reference.display_name.clone(),
                    availability: if printer.virtual_mode == VirtualPrinterMode::Offline {
                        PrinterAvailability::Offline
                    } else {
                        PrinterAvailability::Online
                    },
                })
                .collect();
            providers.push(Arc::new(VirtualPrinterProvider::new(virtual_printers)));
        }

        let provider_names = providers
            .iter()
            .map(|provider| provider.name().to_owned())
            .collect::<Vec<_>>();
        let manager = DiscoveryManager::new(providers, DISCOVERY_TIMEOUT);
        let discovered = manager.discover().await;
        let scanned_at = Utc::now().to_rfc3339();

        let failure_by_provider = discovered
            .failures
            .iter()
            .map(|failure| (failure.provider.as_str(), failure.message.as_str()))
            .collect::<BTreeMap<_, _>>();
        let mut statuses = Vec::new();
        for provider in provider_names {
            let count = discovered
                .printers
                .iter()
                .filter(|printer| {
                    printer
                        .providers
                        .iter()
                        .any(|metadata| metadata.provider == provider)
                })
                .count();
            let failure = failure_by_provider.get(provider.as_str()).copied();
            statuses.push(DiscoveryProviderStatus {
                name: display_provider_name(&provider).to_owned(),
                available: failure.is_none(),
                last_scan_at: Some(scanned_at.clone()),
                detail: failure.map_or_else(
                    || format!("{count} printer(s) observed"),
                    crate::error::sanitize,
                ),
            });
        }
        *self.discovery_status.write().await = statuses;

        let failure_count = discovered.failures.len();
        let _mutation = self.mutation.lock().await;
        let mut next = self.state.read().await.clone();
        let suppressed = next
            .suppressed_system_ids
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        for existing in &mut next.printers {
            if matches!(
                existing.reference.connection,
                PrinterConnection::SystemQueue { .. }
            ) {
                existing.availability = PrinterAvailability::Offline;
            }
        }
        for observation in discovered.printers {
            let id = observation
                .id
                .clone()
                .unwrap_or_else(|| discovered_printer_id(&observation));
            if suppressed.contains(id.as_str()) {
                continue;
            }
            if let Some(index) = matching_printer_index(&next.printers, &id, &observation) {
                let existing = &mut next.printers[index];
                existing.source_name.clone_from(&observation.name);
                existing
                    .reference
                    .connection
                    .clone_from(&observation.connection);
                existing.availability = observation.availability;
                existing.capabilities.clone_from(&observation.capabilities);
                let mut fingerprint = observation.fingerprint.clone();
                fingerprint.merge_missing(&existing.fingerprint);
                existing.fingerprint = fingerprint;
                existing.providers.clone_from(&observation.providers);
                if existing.is_virtual() {
                    existing.capabilities =
                        Some(virtual_capabilities(existing.virtual_width.unwrap_or(80)));
                }
            } else {
                let virtual_width =
                    matches!(observation.connection, PrinterConnection::Virtual { .. })
                        .then_some(80);
                let mut capabilities = observation.capabilities.clone();
                if let Some(width) = virtual_width {
                    capabilities = Some(virtual_capabilities(width));
                }
                next.printers.push(CatalogPrinter {
                    reference: PrinterRef {
                        id,
                        display_name: observation.name.clone(),
                        connection: observation.connection,
                        enabled: true,
                    },
                    source_name: observation.name,
                    availability: observation.availability,
                    capabilities,
                    fingerprint: observation.fingerprint,
                    providers: observation.providers,
                    virtual_width,
                    virtual_mode: VirtualPrinterMode::AlwaysSucceed,
                    virtual_delay_ms: 0,
                });
            }
        }
        self.persist_state(&next).await?;
        let printers = next.printers.clone();
        *self.state.write().await = next;

        for printer in &printers {
            if self.is_active(printer) {
                self.resolver
                    .upsert(printer.reference.clone())
                    .await
                    .map_err(|error| CommandError::internal(error.to_string()))?;
            } else {
                self.resolver.remove(&printer.reference.id).await;
            }
        }
        self.bump_revision();
        self.log.info(
            "discovery",
            format!("Printer discovery completed with {failure_count} provider failure(s)."),
        );
        self.list().await
    }

    pub async fn configure(
        &self,
        printer_id: &str,
        changes: ConfigurePrinterChanges,
    ) -> Result<PrinterSummary, CommandError> {
        let id = PrinterId::new(printer_id.to_owned())
            .map_err(|error| CommandError::invalid(error.to_string()))?;
        let _mutation = self.mutation.lock().await;
        let mut next = self.state.read().await.clone();
        let updated = {
            let printer = next
                .printers
                .iter_mut()
                .find(|printer| printer.reference.id == id)
                .ok_or_else(|| CommandError::not_found("Printer"))?;
            if !feature_allows_printer(self.features, printer) {
                return Err(CommandError::new(
                    "feature_unavailable",
                    "This printer connection is disabled in the current product build.",
                ));
            }
            if let Some(display_name) = changes.display_name {
                printer.reference.display_name = display_name;
            }
            if let Some(enabled) = changes.enabled {
                printer.reference.enabled = enabled;
            }
            printer
                .reference
                .validate()
                .map_err(|error| CommandError::invalid(error.to_string()))?;
            printer.clone()
        };
        self.persist_state(&next).await?;
        *self.state.write().await = next;
        self.resolver
            .upsert(updated.reference.clone())
            .await
            .map_err(|error| CommandError::internal(error.to_string()))?;
        self.bump_revision();
        self.summary(&updated).await
    }

    pub async fn add_manual(
        &self,
        input: ManualPrinterInput,
    ) -> Result<PrinterSummary, CommandError> {
        if !self.features.network_printer_discovery {
            return Err(CommandError::new(
                "feature_unavailable",
                "Manual network printers are disabled in this product build.",
            ));
        }
        let id = PrinterId::new(format!("printer_network_{}", Uuid::new_v4()))
            .map_err(|error| CommandError::internal(error.to_string()))?;
        let reference = PrinterRef {
            id,
            display_name: input.display_name,
            connection: PrinterConnection::Network {
                host: input.host,
                port: input.port,
            },
            enabled: true,
        };
        reference
            .validate()
            .map_err(|error| CommandError::invalid(error.to_string()))?;
        let fingerprint = fingerprint_for_connection(&reference.connection);
        let printer = CatalogPrinter {
            source_name: "Raw TCP printer".to_owned(),
            providers: vec![ProviderMetadata {
                provider: "manual-network".to_owned(),
                provider_id: Some(reference.id.to_string()),
                attributes: BTreeMap::new(),
            }],
            reference,
            availability: PrinterAvailability::Unknown,
            capabilities: Some(receipt_capabilities()),
            fingerprint,
            virtual_width: None,
            virtual_mode: VirtualPrinterMode::AlwaysSucceed,
            virtual_delay_ms: 0,
        };
        let _mutation = self.mutation.lock().await;
        let mut next = self.state.read().await.clone();
        next.printers.push(printer.clone());
        self.persist_state(&next).await?;
        *self.state.write().await = next;
        self.resolver
            .upsert(printer.reference.clone())
            .await
            .map_err(|error| CommandError::internal(error.to_string()))?;
        self.bump_revision();
        self.summary(&printer).await
    }

    pub async fn create_virtual(
        &self,
        input: VirtualPrinterInput,
    ) -> Result<PrinterSummary, CommandError> {
        if !self.features.virtual_printer {
            return Err(CommandError::new(
                "feature_unavailable",
                "Virtual printers are disabled in this product build.",
            ));
        }
        if !matches!(input.width, 58 | 80) {
            return Err(CommandError::invalid(
                "Virtual printer width must be 58 or 80 millimetres.",
            ));
        }
        let id = PrinterId::new(format!("printer_virtual_{}", Uuid::new_v4()))
            .map_err(|error| CommandError::internal(error.to_string()))?;
        let reference = PrinterRef {
            id: id.clone(),
            display_name: input.display_name,
            connection: PrinterConnection::Virtual {
                printer_id: id.to_string(),
            },
            enabled: true,
        };
        reference
            .validate()
            .map_err(|error| CommandError::invalid(error.to_string()))?;
        let fingerprint = fingerprint_for_connection(&reference.connection);
        let printer = CatalogPrinter {
            source_name: "Virtual printer".to_owned(),
            providers: vec![ProviderMetadata {
                provider: "virtual".to_owned(),
                provider_id: Some(reference.id.to_string()),
                attributes: BTreeMap::new(),
            }],
            reference,
            availability: PrinterAvailability::Online,
            capabilities: Some(virtual_capabilities(input.width)),
            fingerprint,
            virtual_width: Some(input.width),
            virtual_mode: VirtualPrinterMode::AlwaysSucceed,
            virtual_delay_ms: 0,
        };
        let _mutation = self.mutation.lock().await;
        let mut next = self.state.read().await.clone();
        next.printers.push(printer.clone());
        self.persist_state(&next).await?;
        *self.state.write().await = next;
        self.virtual_spooler
            .register(id, VirtualPrinterMode::AlwaysSucceed, 0)
            .await;
        self.resolver
            .upsert(printer.reference.clone())
            .await
            .map_err(|error| CommandError::internal(error.to_string()))?;
        self.bump_revision();
        self.summary(&printer).await
    }

    pub async fn update_virtual(
        &self,
        printer_id: &str,
        mode: VirtualPrinterMode,
        delay_ms: u64,
    ) -> Result<PrinterSummary, CommandError> {
        if !self.features.virtual_printer {
            return Err(CommandError::new(
                "feature_unavailable",
                "Virtual printers are disabled in this product build.",
            ));
        }
        if delay_ms > MAX_VIRTUAL_DELAY_MS {
            return Err(CommandError::invalid(format!(
                "Virtual delay must not exceed {MAX_VIRTUAL_DELAY_MS} milliseconds."
            )));
        }
        let id = PrinterId::new(printer_id.to_owned())
            .map_err(|error| CommandError::invalid(error.to_string()))?;
        let _mutation = self.mutation.lock().await;
        let mut next = self.state.read().await.clone();
        let updated = {
            let printer = next
                .printers
                .iter_mut()
                .find(|printer| printer.reference.id == id)
                .ok_or_else(|| CommandError::not_found("Printer"))?;
            if !printer.is_virtual() {
                return Err(CommandError::invalid("Printer is not virtual."));
            }
            printer.virtual_mode = persisted_virtual_mode(mode);
            printer.virtual_delay_ms = delay_ms;
            printer.availability = if mode == VirtualPrinterMode::Offline {
                PrinterAvailability::Offline
            } else {
                PrinterAvailability::Online
            };
            printer.clone()
        };
        self.persist_state(&next).await?;
        *self.state.write().await = next;
        self.virtual_spooler.register(id, mode, delay_ms).await;
        self.bump_revision();
        self.summary(&updated).await
    }

    pub async fn clear_virtual_history(&self, printer_id: &str) -> Result<(), CommandError> {
        if !self.features.virtual_printer {
            return Err(CommandError::new(
                "feature_unavailable",
                "Virtual printers are disabled in this product build.",
            ));
        }
        let id = PrinterId::new(printer_id.to_owned())
            .map_err(|error| CommandError::invalid(error.to_string()))?;
        self.virtual_spooler
            .clear_history(&id)
            .await
            .map_err(|error| CommandError::invalid(error.to_string()))
    }

    pub async fn remove(&self, printer_id: &str) -> Result<(), CommandError> {
        let id = PrinterId::new(printer_id.to_owned())
            .map_err(|error| CommandError::invalid(error.to_string()))?;
        let _mutation = self.mutation.lock().await;
        let mut next = self.state.read().await.clone();
        let removed = {
            let index = next
                .printers
                .iter()
                .position(|printer| printer.reference.id == id)
                .ok_or_else(|| CommandError::not_found("Printer"))?;
            let removed = next.printers.remove(index);
            if matches!(
                removed.reference.connection,
                PrinterConnection::SystemQueue { .. }
            ) && !next
                .suppressed_system_ids
                .iter()
                .any(|suppressed| suppressed == id.as_str())
            {
                next.suppressed_system_ids.push(id.to_string());
            }
            removed
        };
        self.persist_state(&next).await?;
        *self.state.write().await = next;
        self.resolver.remove(&id).await;
        if removed.is_virtual() {
            self.virtual_spooler.remove(&id).await;
        }
        self.bump_revision();
        Ok(())
    }

    pub async fn get(&self, printer_id: &str) -> Result<CatalogPrinter, CommandError> {
        self.state
            .read()
            .await
            .printers
            .iter()
            .find(|printer| printer.reference.id.as_str() == printer_id)
            .cloned()
            .ok_or_else(|| CommandError::not_found("Printer"))
    }

    pub async fn protocol_descriptors(&self) -> Result<Vec<PrinterDescriptor>, CommandError> {
        let state = self.state.read().await;
        state
            .printers
            .iter()
            .filter(|printer| self.is_active(printer))
            .filter(|printer| {
                !matches!(printer.reference.connection, PrinterConnection::Usb { .. })
            })
            .map(protocol_descriptor)
            .collect()
    }

    pub(crate) fn is_active(&self, printer: &CatalogPrinter) -> bool {
        feature_allows_printer(self.features, printer)
    }

    async fn persist_state(&self, state: &PersistedCatalog) -> Result<(), CommandError> {
        let value = serde_json::to_value(state)
            .map_err(|error| CommandError::internal(error.to_string()))?;
        self.storage
            .set_setting(CATALOG_SETTING, &value)
            .await
            .map_err(|error| CommandError::internal(error.to_string()))
    }

    async fn summary(&self, printer: &CatalogPrinter) -> Result<PrinterSummary, CommandError> {
        let (connection_type, address, document_types) = match &printer.reference.connection {
            PrinterConnection::SystemQueue { queue_name } => (
                PrinterConnectionType::SystemQueue,
                Some(queue_name.clone()),
                vec![DocumentType::EscPos],
            ),
            PrinterConnection::Network { host, port } => (
                PrinterConnectionType::Network,
                Some(format!("{host}:{port}")),
                vec![DocumentType::EscPos],
            ),
            PrinterConnection::Virtual { .. } => (
                PrinterConnectionType::Virtual,
                None,
                vec![DocumentType::Virtual, DocumentType::EscPos],
            ),
            PrinterConnection::Usb {
                vendor_id,
                product_id,
                ..
            } => (
                PrinterConnectionType::Usb,
                Some(format!("{vendor_id:04x}:{product_id:04x}")),
                vec![DocumentType::EscPos],
            ),
        };
        let capabilities = printer.capabilities.as_ref().map(|capabilities| {
            let mut types = document_types;
            if capabilities.raster {
                types.push(DocumentType::Raster);
            }
            PrinterCapabilities {
                widths: capabilities.receipt_widths_mm.clone(),
                document_types: types,
                supports_cut: capabilities.cut,
                supports_qr: capabilities.qr_code,
            }
        });
        let (mode, delay_ms, history) = if printer.is_virtual() {
            let (mode, delay_ms) = self
                .virtual_spooler
                .policy(&printer.reference.id)
                .await
                .map_err(|error| CommandError::internal(error.to_string()))?;
            let history = self
                .virtual_spooler
                .history(&printer.reference.id)
                .await
                .map_err(|error| CommandError::internal(error.to_string()))?
                .into_iter()
                .enumerate()
                .map(|(index, submission)| {
                    let (format, preview) = match &submission.document {
                        RenderedDocument::Virtual(document) => (
                            VirtualOutputFormat::Structured,
                            document.preview_lines.join("\n"),
                        ),
                        RenderedDocument::EscPos(bytes) => {
                            (VirtualOutputFormat::EscPos, esc_pos_preview(bytes))
                        }
                        RenderedDocument::Raster(document) => (
                            VirtualOutputFormat::Raster,
                            format!("{} raster page(s)", document.pages.len()),
                        ),
                        RenderedDocument::Native(document) => (
                            VirtualOutputFormat::Structured,
                            format!("Native document ({})", document.media_type),
                        ),
                    };
                    VirtualOutput {
                        id: format!("output_{}_{}", submission.job_id, index),
                        job_id: submission.job_id.to_string(),
                        created_at: submission.recorded_at.to_string(),
                        format,
                        preview,
                        byte_length: submission.document.byte_len(),
                    }
                })
                .collect();
            (Some(mode), Some(delay_ms), Some(history))
        } else {
            (None, None, None)
        };
        Ok(PrinterSummary {
            id: printer.reference.id.to_string(),
            display_name: printer.reference.display_name.clone(),
            source_name: printer.source_name.clone(),
            connection_type,
            address,
            enabled: printer.reference.enabled && self.is_active(printer),
            available: self.is_active(printer)
                && matches!(printer.availability, PrinterAvailability::Online),
            is_virtual: printer.is_virtual(),
            capabilities,
            mode,
            delay_ms,
            history,
        })
    }
}

fn feature_allows_printer(features: ProductFeatures, printer: &CatalogPrinter) -> bool {
    match &printer.reference.connection {
        PrinterConnection::SystemQueue { .. } => features.system_printer_discovery,
        PrinterConnection::Network { .. } => features.network_printer_discovery,
        PrinterConnection::Usb { .. } => features.usb_printer_discovery,
        PrinterConnection::Virtual { .. } => features.virtual_printer,
    }
}

fn persisted_virtual_mode(mode: VirtualPrinterMode) -> VirtualPrinterMode {
    if mode == VirtualPrinterMode::FailNext {
        VirtualPrinterMode::AlwaysSucceed
    } else {
        mode
    }
}

fn matching_printer_index(
    printers: &[CatalogPrinter],
    observed_id: &PrinterId,
    observation: &DiscoveredPrinter,
) -> Option<usize> {
    if let Some(index) = printers
        .iter()
        .position(|printer| printer.reference.id == *observed_id)
    {
        return Some(index);
    }

    let observed_keys = observation
        .fingerprint
        .identity_keys()
        .into_iter()
        .collect::<BTreeSet<_>>();
    printers.iter().position(|printer| {
        (!observed_keys.is_empty()
            && printer
                .fingerprint
                .identity_keys()
                .iter()
                .any(|key| observed_keys.contains(key)))
            || printer.providers.iter().any(|stored| {
                observation.providers.iter().any(|observed| {
                    stored.provider == observed.provider
                        && stored.provider_id.is_some()
                        && stored.provider_id == observed.provider_id
                })
            })
    })
}

fn fingerprint_for_connection(connection: &PrinterConnection) -> PrinterFingerprint {
    match connection {
        PrinterConnection::SystemQueue { queue_name } => PrinterFingerprint {
            queue_name: Some(queue_name.clone()),
            ..PrinterFingerprint::default()
        },
        PrinterConnection::Network { host, port } => PrinterFingerprint {
            host: Some(host.clone()),
            port: Some(*port),
            ..PrinterFingerprint::default()
        },
        PrinterConnection::Usb {
            vendor_id,
            product_id,
            serial_number,
        } => PrinterFingerprint {
            usb_vendor_id: Some(*vendor_id),
            usb_product_id: Some(*product_id),
            serial_number: serial_number.clone(),
            ..PrinterFingerprint::default()
        },
        PrinterConnection::Virtual { .. } => PrinterFingerprint::default(),
    }
}

fn esc_pos_preview(bytes: &[u8]) -> String {
    let shown = bytes.len().min(MAX_ESC_POS_PREVIEW_BYTES);
    let mut lines = bytes[..shown]
        .chunks(16)
        .map(|chunk| {
            chunk
                .iter()
                .map(|byte| format!("{byte:02X}"))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>();
    if shown < bytes.len() {
        lines.push(format!("... {} more byte(s)", bytes.len() - shown));
    }
    lines.join("\n")
}

fn protocol_descriptor(printer: &CatalogPrinter) -> Result<PrinterDescriptor, CommandError> {
    let (kind, connection, fingerprint) = match &printer.reference.connection {
        PrinterConnection::SystemQueue { queue_name } => (
            ProtocolPrinterKind::Local,
            ProtocolConnection::System {
                system_name: queue_name.clone(),
            },
            format!("system:{queue_name}"),
        ),
        PrinterConnection::Network { host, port } => (
            ProtocolPrinterKind::Network,
            ProtocolConnection::Tcp {
                host: host.clone(),
                port: *port,
            },
            format!("tcp:{host}:{port}"),
        ),
        PrinterConnection::Virtual { printer_id } => (
            ProtocolPrinterKind::Virtual,
            ProtocolConnection::Virtual,
            format!("virtual:{printer_id}"),
        ),
        PrinterConnection::Usb { .. } => {
            return Err(CommandError::new(
                "feature_unavailable",
                "The current wire protocol cannot advertise USB printer descriptors.",
            ));
        }
    };
    Ok(PrinterDescriptor {
        id: printer.reference.id.to_string(),
        fingerprint: protocol_fingerprint(printer).unwrap_or(fingerprint),
        name: printer.reference.display_name.clone(),
        kind,
        connection,
        capabilities: printer
            .capabilities
            .as_ref()
            .and_then(protocol_capabilities),
        enabled: printer.reference.enabled,
        availability: match printer.availability {
            PrinterAvailability::Online => ProtocolAvailability::Online,
            PrinterAvailability::Offline => ProtocolAvailability::Offline,
            PrinterAvailability::Unknown | PrinterAvailability::Degraded => {
                ProtocolAvailability::Unknown
            }
        },
    })
}

fn protocol_capabilities(capabilities: &DomainPrinterCapabilities) -> Option<ProtocolCapabilities> {
    let mut widths = capabilities
        .receipt_widths_mm
        .iter()
        .filter_map(|width| match width {
            58 => Some(ReceiptWidth::Mm58),
            80 => Some(ReceiptWidth::Mm80),
            _ => None,
        })
        .collect::<Vec<_>>();
    widths.sort_by_key(|width| width.millimetres());
    widths.dedup();
    (!widths.is_empty()).then_some(ProtocolCapabilities {
        media_widths: widths,
        raster: capabilities.raster,
        cut: capabilities.cut,
        qr: capabilities.qr_code,
        barcode: capabilities.barcode,
    })
}

fn protocol_fingerprint(printer: &CatalogPrinter) -> Option<String> {
    printer
        .fingerprint
        .identity_keys()
        .into_iter()
        .next()
        .or_else(|| {
            printer.providers.iter().find_map(|provider| {
                provider
                    .provider_id
                    .as_ref()
                    .map(|provider_id| format!("{}:{provider_id}", provider.provider))
            })
        })
}

fn discovered_printer_id(printer: &DiscoveredPrinter) -> PrinterId {
    let source = printer
        .fingerprint
        .identity_keys()
        .into_iter()
        .next()
        .or_else(|| {
            printer.providers.iter().find_map(|provider| {
                provider
                    .provider_id
                    .as_ref()
                    .map(|provider_id| format!("{}:{provider_id}", provider.provider))
            })
        })
        .unwrap_or_else(|| connection_fingerprint(&printer.connection));
    PrinterId::new(format!(
        "printer_discovered_{:016x}",
        fnv1a(source.as_bytes())
    ))
    .expect("bounded generated printer id")
}

fn connection_fingerprint(connection: &PrinterConnection) -> String {
    match connection {
        PrinterConnection::SystemQueue { queue_name } => format!("system:{queue_name}"),
        PrinterConnection::Network { host, port } => format!("tcp:{host}:{port}"),
        PrinterConnection::Usb {
            vendor_id,
            product_id,
            serial_number,
        } => format!(
            "usb:{vendor_id}:{product_id}:{}",
            serial_number.as_deref().unwrap_or("")
        ),
        PrinterConnection::Virtual { printer_id } => format!("virtual:{printer_id}"),
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn receipt_capabilities() -> DomainPrinterCapabilities {
    DomainPrinterCapabilities {
        receipt_widths_mm: vec![58, 80],
        esc_pos: true,
        raster: false,
        cut: true,
        qr_code: true,
        barcode: true,
        cancellation: true,
    }
}

fn virtual_capabilities(width: u16) -> DomainPrinterCapabilities {
    DomainPrinterCapabilities {
        receipt_widths_mm: vec![width],
        raster: false,
        ..receipt_capabilities()
    }
}

fn display_provider_name(provider: &str) -> &str {
    match provider {
        "system-queue" => "System queues",
        "manual-network" => "Manual network",
        "virtual" => "Virtual",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use oppa_core::PrinterId;
    use oppa_printer::{
        DiscoveredPrinter, PrinterAvailability, PrinterConnection, PrinterFingerprint, PrinterKind,
        PrinterRef, ProviderMetadata,
    };
    use oppa_product::ProductFeatures;

    use super::{discovered_printer_id, esc_pos_preview, feature_allows_printer};
    use crate::models::{CatalogPrinter, VirtualPrinterMode};

    #[test]
    fn discovery_identity_is_stable_for_the_same_connection() {
        let printer = DiscoveredPrinter {
            id: None,
            name: "Receipt".to_owned(),
            kind: PrinterKind::Receipt,
            connection: PrinterConnection::SystemQueue {
                queue_name: "receipt".to_owned(),
            },
            fingerprint: PrinterFingerprint::default(),
            availability: PrinterAvailability::Unknown,
            capabilities: None,
            providers: vec![ProviderMetadata {
                provider: "test".to_owned(),
                provider_id: None,
                attributes: BTreeMap::default(),
            }],
        };

        assert_eq!(
            discovered_printer_id(&printer),
            discovered_printer_id(&printer)
        );
    }

    #[test]
    fn disabled_product_feature_excludes_persisted_printer() {
        let printer = CatalogPrinter {
            reference: PrinterRef {
                id: PrinterId::new("printer_virtual_test").expect("valid id"),
                display_name: "Virtual".to_owned(),
                connection: PrinterConnection::Virtual {
                    printer_id: "virtual_test".to_owned(),
                },
                enabled: true,
            },
            source_name: "Virtual printer".to_owned(),
            availability: PrinterAvailability::Online,
            capabilities: None,
            fingerprint: PrinterFingerprint::default(),
            providers: Vec::new(),
            virtual_width: Some(80),
            virtual_mode: VirtualPrinterMode::AlwaysSucceed,
            virtual_delay_ms: 0,
        };

        assert!(!feature_allows_printer(
            ProductFeatures::default(),
            &printer
        ));
        assert!(feature_allows_printer(
            ProductFeatures {
                virtual_printer: true,
                ..ProductFeatures::default()
            },
            &printer
        ));
    }

    #[test]
    fn esc_pos_preview_is_hex_encoded_and_bounded() {
        assert_eq!(esc_pos_preview(&[0x1b, 0x40, 0x0a]), "1B 40 0A");

        let bytes = vec![0xFF; 513];
        let preview = esc_pos_preview(&bytes);
        assert!(preview.ends_with("... 1 more byte(s)"));
        assert_eq!(preview.matches("FF").count(), 512);
    }
}
