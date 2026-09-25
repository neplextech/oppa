use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::Utc;
use oppa_agent::StaticPrinterResolver;
use oppa_core::PrinterId;
use oppa_discovery::{
    DiscoveryManager, DiscoveryProvider, ManualNetworkPrinter, ManualNetworkProvider,
    SystemQueueProvider, VirtualPrinterDefinition, VirtualPrinterProvider,
};
use oppa_printer::{
    DiscoveredPrinter, PrinterAvailability, PrinterCapabilities as DomainPrinterCapabilities,
    PrinterConnection, PrinterFingerprint, PrinterLanguage, PrinterRef, ProviderMetadata,
    SubmissionMode, VirtualPrinterProfile,
};
use oppa_product::ProductFeatures;
use oppa_protocol::{
    PrinterAvailability as ProtocolAvailability, PrinterCapabilities as ProtocolCapabilities,
    PrinterConnection as ProtocolConnection, PrinterDescriptor, PrinterKind as ProtocolPrinterKind,
    ReceiptWidth,
};
use oppa_renderer::{RenderedDocument, encode_raster_page_preview_png};
use oppa_spooler::VirtualSubmission;
use oppa_storage::SqliteStorage;
use serde_json::Value;
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::{
    diagnostics::DiagnosticLog,
    error::CommandError,
    models::{
        CatalogPrinter, ConfigurePrinterChanges, DiscoveryProviderStatus, DocumentType,
        ManualPrinterInput, PersistedCatalog, PrinterCapabilities, PrinterConnectionType,
        PrinterOutputModes, PrinterReceiptFeatures, PrinterSummary, VirtualOutput,
        VirtualOutputDiagnostics, VirtualOutputFormat, VirtualPrinterInput, VirtualPrinterMode,
    },
    virtual_spooler::PerPrinterVirtualSpooler,
};

const CATALOG_SETTING: &str = "desktop.printer-catalog.v1";
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_VIRTUAL_DELAY_MS: u64 = 60_000;

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
            Some(mut value) => {
                let migrated = migrate_legacy_printer_config(&mut value);
                let persisted =
                    serde_json::from_value::<PersistedCatalog>(value).map_err(|error| {
                        CommandError::internal(format!("printer catalog is invalid: {error}"))
                    })?;
                if migrated {
                    let migrated_value = serde_json::to_value(&persisted)
                        .map_err(|error| CommandError::internal(error.to_string()))?;
                    storage
                        .set_setting(CATALOG_SETTING, &migrated_value)
                        .await
                        .map_err(|error| CommandError::internal(error.to_string()))?;
                }
                persisted
            }
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
                    existing.capabilities = Some(virtual_capabilities_for_profile(
                        existing.reference.virtual_profile,
                    ));
                }
            } else {
                let is_virtual =
                    matches!(observation.connection, PrinterConnection::Virtual { .. });
                let default_virtual_profile =
                    is_virtual.then_some(VirtualPrinterProfile::EscPosReceipt { width_mm: 80 });
                let submission_mode = new_printer_submission_mode(
                    &observation.connection,
                    observation.capabilities.as_ref(),
                );
                let virtual_profile = default_virtual_profile;
                let virtual_width = virtual_profile.and_then(|profile| match profile {
                    VirtualPrinterProfile::EscPosReceipt { width_mm } => Some(width_mm),
                    VirtualPrinterProfile::SystemDriverPage { .. } => None,
                });
                let mut capabilities = observation.capabilities.clone();
                if let Some(profile) = virtual_profile {
                    capabilities = Some(virtual_capabilities_for_profile(Some(profile)));
                } else if matches!(
                    observation.connection,
                    PrinterConnection::SystemQueue { .. }
                ) && capabilities.is_none()
                {
                    capabilities = Some(system_driver_capabilities());
                }
                next.printers.push(CatalogPrinter {
                    reference: PrinterRef {
                        id,
                        display_name: observation.name.clone(),
                        connection: observation.connection,
                        submission_mode,
                        virtual_profile,
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
            if let Some(submission_mode) = changes.submission_mode {
                if !matches!(
                    printer.reference.connection,
                    PrinterConnection::SystemQueue { .. }
                ) {
                    return Err(CommandError::invalid(
                        "Only system queues can switch between driver and raw ESC/POS mode.",
                    ));
                }
                printer.reference.submission_mode = submission_mode;
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
            submission_mode: SubmissionMode::Raw(PrinterLanguage::EscPos),
            virtual_profile: None,
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
        input
            .profile
            .validate()
            .map_err(|error| CommandError::invalid(error.to_string()))?;
        let id = PrinterId::new(format!("printer_virtual_{}", Uuid::new_v4()))
            .map_err(|error| CommandError::internal(error.to_string()))?;
        let reference = PrinterRef {
            id: id.clone(),
            display_name: input.display_name,
            connection: PrinterConnection::Virtual {
                printer_id: id.to_string(),
            },
            submission_mode: mode_for_virtual_profile(input.profile),
            virtual_profile: Some(input.profile),
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
            capabilities: Some(virtual_capabilities_for_profile(Some(input.profile))),
            fingerprint,
            virtual_width: match input.profile {
                VirtualPrinterProfile::EscPosReceipt { width_mm } => Some(width_mm),
                VirtualPrinterProfile::SystemDriverPage { .. } => None,
            },
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
        let (connection_type, address) = match &printer.reference.connection {
            PrinterConnection::SystemQueue { queue_name } => {
                (PrinterConnectionType::SystemQueue, Some(queue_name.clone()))
            }
            PrinterConnection::Network { host, port } => (
                PrinterConnectionType::Network,
                Some(format!("{host}:{port}")),
            ),
            PrinterConnection::Virtual { .. } => (PrinterConnectionType::Virtual, None),
            PrinterConnection::Usb {
                vendor_id,
                product_id,
                ..
            } => (
                PrinterConnectionType::Usb,
                Some(format!("{vendor_id:04x}:{product_id:04x}")),
            ),
        };
        let capabilities = printer.capabilities.as_ref().map(|capabilities| {
            let mut types = configured_document_types(printer);
            if capabilities.raster {
                types.push(DocumentType::Raster);
            }
            PrinterCapabilities {
                widths: capabilities.receipt_widths_mm.clone(),
                document_types: types,
                output_modes: PrinterOutputModes {
                    supports_system_driver: capabilities.system_driver,
                    supports_esc_pos: capabilities.esc_pos
                        || matches!(
                            printer.reference.submission_mode,
                            SubmissionMode::Raw(PrinterLanguage::EscPos)
                        ),
                },
                receipt_features: PrinterReceiptFeatures {
                    supports_cut: capabilities.cut,
                    supports_qr: capabilities.qr_code,
                },
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
                .map(virtual_output)
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
            submission_mode: printer.reference.submission_mode,
            virtual_profile: printer.reference.virtual_profile,
            mode,
            delay_ms,
            history,
        })
    }
}

fn virtual_output((index, submission): (usize, VirtualSubmission)) -> VirtualOutput {
    let (format, preview, document) = match &submission.document {
        RenderedDocument::Virtual(document) => (
            VirtualOutputFormat::Structured,
            document.preview_lines.join("\n"),
            Some(document.document.clone()),
        ),
        RenderedDocument::EscPos(bytes) => {
            let diagnostics = submission.preview.escpos.as_ref();
            (
                VirtualOutputFormat::EscPos,
                diagnostics.map_or_else(
                    || format!("{} ESC/POS bytes interpreted", bytes.bytes.len()),
                    |diagnostics| {
                        format!(
                            "{} commands · {} text lines · {} QR · {} barcode · {} image{} · {} feed lines · cut {}",
                            diagnostics.interpreted_commands,
                            diagnostics.text_lines,
                            diagnostics.qr_codes,
                            diagnostics.barcodes,
                            diagnostics.images,
                            if diagnostics.images == 1 { "" } else { "s" },
                            diagnostics.feed_lines,
                            if diagnostics.cut_requested { "requested" } else { "not requested" },
                        )
                    },
                ),
                None,
            )
        }
        RenderedDocument::Raster(document) => (
            VirtualOutputFormat::Raster,
            format!("{} raster page(s)", document.pages.len()),
            None,
        ),
        RenderedDocument::Native(document) => (
            VirtualOutputFormat::Structured,
            format!("Native document ({})", document.media_type),
            None,
        ),
    };
    let image_data_url = submission
        .preview
        .pages
        .first()
        .and_then(|page| encode_raster_page_preview_png(page).ok())
        .map(|png| format!("data:image/png;base64,{}", STANDARD.encode(png)));
    let diagnostics =
        submission
            .preview
            .escpos
            .as_ref()
            .map(|diagnostics| VirtualOutputDiagnostics {
                interpreted_commands: diagnostics.interpreted_commands,
                text_lines: diagnostics.text_lines,
                images: diagnostics.images,
                qr_codes: diagnostics.qr_codes,
                barcodes: diagnostics.barcodes,
                feed_lines: diagnostics.feed_lines,
                cut_requested: diagnostics.cut_requested,
                unsupported_commands: diagnostics
                    .unsupported_commands
                    .iter()
                    .map(|command| command.command.clone())
                    .collect(),
            });
    VirtualOutput {
        id: format!("output_{}_{}", submission.job_id, index),
        job_id: submission.job_id.to_string(),
        created_at: submission.recorded_at.to_string(),
        format,
        preview,
        byte_length: submission.document.byte_len(),
        image_data_url,
        diagnostics,
        document,
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

/// Adds explicit submission and virtual device profiles to older catalog rows.
///
/// Old system queues are moved to the safe driver path. Existing network and
/// USB targets retain their former raw ESC/POS behavior, and old virtual
/// printers become thermal profiles using their persisted receipt width.
fn migrate_legacy_printer_config(value: &mut Value) -> bool {
    let Some(printers) = value.get_mut("printers").and_then(Value::as_array_mut) else {
        return false;
    };
    let mut changed = false;
    for printer in printers {
        let legacy_width = printer
            .get("virtualWidth")
            .and_then(Value::as_u64)
            .filter(|width| matches!(width, 58 | 80))
            .unwrap_or(80);
        let Some(reference) = printer.get_mut("reference").and_then(Value::as_object_mut) else {
            continue;
        };
        let connection_type = reference
            .get("connection")
            .and_then(|connection| connection.get("type"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let had_submission_mode = reference.contains_key("submissionMode");
        if !had_submission_mode {
            let submission_mode = match connection_type.as_deref() {
                Some("system-queue") => serde_json::json!({ "type": "driver" }),
                Some("network" | "usb" | "virtual") => serde_json::json!({
                    "type": "raw",
                    "language": "esc-pos"
                }),
                _ => continue,
            };
            reference.insert("submissionMode".to_owned(), submission_mode);
            changed = true;
        }
        if connection_type.as_deref() == Some("virtual")
            && !reference.contains_key("virtualProfile")
        {
            let is_driver = reference
                .get("submissionMode")
                .and_then(|mode| mode.get("type"))
                .and_then(Value::as_str)
                == Some("driver");
            let profile = if is_driver {
                serde_json::json!({
                    "type": "system-driver-page",
                    "pageWidthMm": 210,
                    "pageHeightMm": 297,
                    "dpi": 300
                })
            } else {
                serde_json::json!({
                    "type": "esc-pos-receipt",
                    "widthMm": legacy_width
                })
            };
            reference.insert("virtualProfile".to_owned(), profile);
            changed = true;
        }
        if !had_submission_mode && connection_type.as_deref() == Some("system-queue") {
            migrate_legacy_system_queue_capabilities(printer);
            changed = true;
        }
    }
    changed
}

fn migrate_legacy_system_queue_capabilities(printer: &mut Value) {
    let Some(printer) = printer.as_object_mut() else {
        return;
    };
    let capabilities = printer
        .entry("capabilities".to_owned())
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    if capabilities.is_null() {
        *capabilities = Value::Object(serde_json::Map::new());
    }
    let Some(capabilities) = capabilities.as_object_mut() else {
        return;
    };
    capabilities
        .entry("receiptWidthsMm".to_owned())
        .or_insert_with(|| serde_json::json!([58, 80]));
    capabilities.insert("systemDriver".to_owned(), Value::Bool(true));
    capabilities.insert("escPos".to_owned(), Value::Bool(false));
    capabilities.insert("raster".to_owned(), Value::Bool(true));
    capabilities.insert("cut".to_owned(), Value::Bool(false));
    capabilities.insert("qrCode".to_owned(), Value::Bool(true));
    capabilities.insert("barcode".to_owned(), Value::Bool(true));
    capabilities.insert("cancellation".to_owned(), Value::Bool(true));
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
        capabilities: printer.capabilities.as_ref().and_then(|capabilities| {
            protocol_capabilities(capabilities, printer.reference.submission_mode)
        }),
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

fn protocol_capabilities(
    capabilities: &DomainPrinterCapabilities,
    submission_mode: SubmissionMode,
) -> Option<ProtocolCapabilities> {
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
        system_driver: Some(capabilities.system_driver),
        esc_pos: Some(
            capabilities.esc_pos
                || matches!(
                    submission_mode,
                    SubmissionMode::Raw(PrinterLanguage::EscPos)
                ),
        ),
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
        system_driver: false,
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
        system_driver: false,
        esc_pos: true,
        raster: false,
        cut: true,
        qr_code: true,
        barcode: true,
        cancellation: true,
    }
}

fn virtual_capabilities_for_profile(
    profile: Option<VirtualPrinterProfile>,
) -> DomainPrinterCapabilities {
    match profile {
        Some(VirtualPrinterProfile::EscPosReceipt { width_mm }) => virtual_capabilities(width_mm),
        Some(VirtualPrinterProfile::SystemDriverPage { .. }) | None => system_driver_capabilities(),
    }
}

fn system_driver_capabilities() -> DomainPrinterCapabilities {
    DomainPrinterCapabilities {
        receipt_widths_mm: vec![58, 80],
        esc_pos: false,
        system_driver: true,
        raster: true,
        cut: false,
        qr_code: true,
        barcode: true,
        cancellation: true,
    }
}

fn mode_for_virtual_profile(profile: VirtualPrinterProfile) -> SubmissionMode {
    match profile {
        VirtualPrinterProfile::EscPosReceipt { .. } => SubmissionMode::Raw(PrinterLanguage::EscPos),
        VirtualPrinterProfile::SystemDriverPage { .. } => SubmissionMode::Driver,
    }
}

fn new_printer_submission_mode(
    connection: &PrinterConnection,
    capabilities: Option<&DomainPrinterCapabilities>,
) -> SubmissionMode {
    match connection {
        PrinterConnection::SystemQueue { .. } | PrinterConnection::Usb { .. } => {
            SubmissionMode::Driver
        }
        PrinterConnection::Network { .. }
            if capabilities.is_some_and(|capabilities| capabilities.esc_pos) =>
        {
            SubmissionMode::Raw(PrinterLanguage::EscPos)
        }
        PrinterConnection::Virtual { .. } => SubmissionMode::Raw(PrinterLanguage::EscPos),
        PrinterConnection::Network { .. } => SubmissionMode::Driver,
    }
}

fn configured_document_types(printer: &CatalogPrinter) -> Vec<DocumentType> {
    if printer.is_virtual() {
        let mut types = vec![DocumentType::Virtual];
        match printer.reference.virtual_profile {
            Some(VirtualPrinterProfile::EscPosReceipt { .. }) => types.push(DocumentType::EscPos),
            Some(VirtualPrinterProfile::SystemDriverPage { .. }) => {
                types.push(DocumentType::Driver);
            }
            None => {}
        }
        return types;
    }
    match printer.reference.submission_mode {
        SubmissionMode::Driver => vec![DocumentType::Driver],
        SubmissionMode::Raw(PrinterLanguage::EscPos) => vec![DocumentType::EscPos],
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
        PrinterRef, ProviderMetadata, SubmissionMode, VirtualPrinterProfile,
    };
    use oppa_product::ProductFeatures;

    use super::{
        discovered_printer_id, feature_allows_printer, migrate_legacy_printer_config,
        protocol_capabilities, system_driver_capabilities,
    };
    use crate::models::{CatalogPrinter, PersistedCatalog, VirtualPrinterMode};

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
                submission_mode: SubmissionMode::Driver,
                virtual_profile: Some(VirtualPrinterProfile::SystemDriverPage {
                    page_width_mm: 210,
                    page_height_mm: 297,
                    dpi: 300,
                }),
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
    fn legacy_catalog_migration_uses_safe_system_queue_default_and_preserves_raw_receipts() {
        let mut catalog = serde_json::json!({
            "printers": [
                {
                    "reference": {
                        "id": "printer_office",
                        "displayName": "Office",
                        "connection": { "type": "system-queue", "queue_name": "Office" },
                        "enabled": true
                    },
                    "sourceName": "Office",
                    "availability": "unknown",
                    "capabilities": {
                        "receiptWidthsMm": [58, 80],
                        "escPos": true,
                        "raster": false,
                        "cut": true,
                        "qrCode": true,
                        "barcode": true,
                        "cancellation": true
                    }
                },
                {
                    "reference": {
                        "id": "printer_network",
                        "displayName": "Receipt",
                        "connection": { "type": "network", "host": "127.0.0.1", "port": 9100 },
                        "enabled": true
                    },
                    "sourceName": "Raw TCP printer",
                    "availability": "unknown",
                    "capabilities": null
                },
                {
                    "reference": {
                        "id": "printer_virtual",
                        "displayName": "Virtual thermal",
                    "connection": { "type": "virtual", "printer_id": "virtual" },
                        "enabled": true
                    },
                    "sourceName": "Virtual printer",
                    "availability": "online",
                    "capabilities": null,
                    "virtualWidth": 58
                }
            ]
        });

        assert!(migrate_legacy_printer_config(&mut catalog));
        assert_eq!(
            catalog["printers"][0]["reference"]["submissionMode"],
            serde_json::json!({ "type": "driver" })
        );
        assert_eq!(
            catalog["printers"][1]["reference"]["submissionMode"],
            serde_json::json!({ "type": "raw", "language": "esc-pos" })
        );
        assert_eq!(
            catalog["printers"][2]["reference"]["virtualProfile"],
            serde_json::json!({ "type": "esc-pos-receipt", "widthMm": 58 })
        );

        let migrated: PersistedCatalog =
            serde_json::from_value(catalog.clone()).expect("migrated catalog deserializes");
        assert!(
            migrated
                .printers
                .iter()
                .all(|printer| printer.reference.validate().is_ok())
        );
        let office_capabilities = migrated.printers[0]
            .capabilities
            .as_ref()
            .expect("legacy system queue gains driver capabilities");
        assert!(office_capabilities.system_driver);
        assert!(!office_capabilities.esc_pos);
        assert!(office_capabilities.raster);
        assert!(!office_capabilities.cut);
        assert!(!migrate_legacy_printer_config(&mut catalog));
    }

    #[test]
    fn current_printer_configuration_roundtrips_without_changing_mode_or_profile() {
        let mut catalog = serde_json::json!({
            "printers": [{
                "reference": {
                    "id": "printer_virtual_office",
                    "displayName": "Virtual office",
                    "connection": { "type": "virtual", "printer_id": "virtual-office" },
                    "submissionMode": { "type": "driver" },
                    "virtualProfile": {
                        "type": "system-driver-page",
                        "pageWidthMm": 210,
                        "pageHeightMm": 297,
                        "dpi": 300
                    },
                    "enabled": true
                },
                "sourceName": "Virtual printer",
                "availability": "online",
                "capabilities": null
            }]
        });
        assert!(!migrate_legacy_printer_config(&mut catalog));
        let parsed: PersistedCatalog = serde_json::from_value(catalog).expect("catalog");
        let reference = &parsed.printers[0].reference;
        assert_eq!(reference.submission_mode, SubmissionMode::Driver);
        assert_eq!(
            reference.virtual_profile,
            Some(VirtualPrinterProfile::SystemDriverPage {
                page_width_mm: 210,
                page_height_mm: 297,
                dpi: 300,
            })
        );
    }

    #[test]
    fn generic_system_queue_capabilities_claim_escpos_only_after_explicit_configuration() {
        let capabilities = system_driver_capabilities();
        let driver = protocol_capabilities(&capabilities, SubmissionMode::Driver)
            .expect("driver capabilities");
        assert_eq!(driver.system_driver, Some(true));
        assert_eq!(driver.esc_pos, Some(false));

        let raw = protocol_capabilities(
            &capabilities,
            SubmissionMode::Raw(oppa_printer::PrinterLanguage::EscPos),
        )
        .expect("explicit raw capabilities");
        assert_eq!(raw.system_driver, Some(true));
        assert_eq!(raw.esc_pos, Some(true));
    }
}
