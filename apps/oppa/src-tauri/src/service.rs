use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use chrono::Utc;
use oppa_agent::{Agent, AgentBuilder, AgentHandle, AgentSnapshot, AgentState, ProcessOutcome};
use oppa_auth::{AgentKeyManager, PairingClient};
use oppa_core::Timestamp;
use oppa_platform::{
    AppPaths, BrowserOpener, CredentialStore, KeyringCredentialStore, SystemBrowser, platform_info,
    resolve_app_paths,
};
use oppa_product::{ProductConfig, embedded_product};
use oppa_protocol::{
    AgentMessageKind, PrintDocument, PrintJob, PrintSection, ReceiptWidth, TextAlignment,
};
use oppa_spooler::{RawTcpSpooler, SpoolerRegistry, SystemQueueSpooler};
use oppa_storage::{DEFAULT_MAX_PENDING_JOBS, JobRepository, SqliteStorage};
use tauri::{AppHandle, Emitter};
use tokio::sync::{Mutex, RwLock, mpsc};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    connection::{
        ConnectionControl, OutboundChannelReporter, OutboundRequest, run_connection_supervisor,
    },
    diagnostics::DiagnosticLog,
    error::{CommandError, sanitize},
    job_ledger::JobLedger,
    models::{
        AgentStatus, ConnectedServiceSummary, DesktopJobState, DiagnosticExport, Diagnostics,
        DiscoveredServiceSummary, FeatureAvailability, JobSummary, ManualPrinterInput,
        OpenPrinterConnectionState, PrinterSummary, ProductLink, ProductSummary, RecentServer,
        VirtualPrinterInput, VirtualPrinterMode,
    },
    printer_catalog::PrinterCatalog,
    server_configuration::{
        CONNECTION_SETTING, LEGACY_SERVER_CONFIGURATION_SETTING, OpenPrinterConnection,
        OpenPrinterServerConfiguration, OpenPrinterServerConfigurationInput,
        RECENT_SERVERS_SETTING, SERVER_CONFIGURATION_SETTING, migrate_legacy_configuration,
    },
    virtual_spooler::PerPrinterVirtualSpooler,
};

use oppa_core::PrinterId;

pub const STATE_CHANGED_EVENT: &str = "oppa://state-changed";
pub const PRINTERS_CHANGED_EVENT: &str = "oppa://printers-changed";
pub const JOBS_CHANGED_EVENT: &str = "oppa://jobs-changed";

const OUTBOUND_CHANNEL_CAPACITY: usize = 256;

/// Long-lived desktop host for the shell-independent agent runtime.
pub struct DesktopService {
    pub(crate) app: AppHandle,
    pub(crate) product: ProductConfig,
    pub(crate) storage: SqliteStorage,
    pub(crate) key_manager: AgentKeyManager,
    pub(crate) agent: Arc<Agent>,
    pub(crate) catalog: Arc<PrinterCatalog>,
    pub(crate) jobs: Arc<JobLedger>,
    pub(crate) log: Arc<DiagnosticLog>,
    pub(crate) started_at: Instant,
    pub(crate) last_connection_at: RwLock<Option<String>>,
    pub(crate) agent_id: RwLock<Option<String>>,
    pub(crate) server_configuration: RwLock<OpenPrinterServerConfiguration>,
    pub(crate) connection: RwLock<Option<OpenPrinterConnection>>,
    pub(crate) connection_state: RwLock<OpenPrinterConnectionState>,
    pub(crate) connected_service: RwLock<Option<ConnectedServiceSummary>>,
    pub(crate) shutdown: CancellationToken,
    pub(crate) startup_recovered_submissions: usize,
    paths: AppPaths,
    pub(crate) pairing_client: PairingClient,
    pub(crate) provider_operation: Mutex<()>,
    pub(crate) provider_generation: AtomicU64,
    connection_control: mpsc::Sender<ConnectionControl>,
    virtual_spooler: Arc<PerPrinterVirtualSpooler>,
}

impl DesktopService {
    #[allow(clippy::too_many_lines)]
    pub async fn initialize(app: AppHandle) -> Result<Arc<Self>, CommandError> {
        let product = embedded_product()
            .map_err(|error| CommandError::internal(error.to_string()))?
            .clone();
        let paths = resolve_paths(&product)?;
        let storage = SqliteStorage::open(
            paths.data_dir.join("oppa.sqlite3"),
            DEFAULT_MAX_PENDING_JOBS,
        )
        .await
        .map_err(|error| CommandError::internal(error.to_string()))?;
        let recovery = storage
            .recover_interrupted()
            .await
            .map_err(|error| CommandError::internal(error.to_string()))?;
        let startup_pending = storage
            .pending()
            .await
            .map_err(|error| CommandError::internal(error.to_string()))?;
        let log = Arc::new(DiagnosticLog::default());
        log.info(
            "application",
            format!("{} desktop runtime is starting.", product.product_name),
        );
        let mut migrated_legacy = false;
        let stored_configuration = storage
            .setting(SERVER_CONFIGURATION_SETTING)
            .await
            .map_err(|error| CommandError::internal(error.to_string()))?;
        let has_stored_configuration = stored_configuration.is_some();
        let server_configuration = if let Some(value) = stored_configuration {
            serde_json::from_value::<OpenPrinterServerConfiguration>(value)
                .ok()
                .filter(|configuration| configuration.validate().is_ok())
                .unwrap_or_default()
        } else if let Some(legacy) = storage
            .setting(LEGACY_SERVER_CONFIGURATION_SETTING)
            .await
            .map_err(|error| CommandError::internal(error.to_string()))?
        {
            migrated_legacy = true;
            migrate_legacy_configuration(&legacy)
                .and_then(|migration| migration.suggested)
                .unwrap_or_default()
        } else {
            OpenPrinterServerConfiguration::default()
        };

        let credential_store: Arc<dyn CredentialStore> = Arc::new(
            KeyringCredentialStore::new(product.application_id.clone())
                .map_err(|error| CommandError::internal(error.to_string()))?,
        );
        let key_manager = AgentKeyManager::new(Arc::clone(&credential_store));
        if migrated_legacy {
            credential_store
                .delete("oauth-token-set-v1")
                .await
                .map_err(|error| CommandError::new("credential_clear_failed", error.to_string()))?;
            let value = serde_json::to_value(&server_configuration)
                .map_err(|error| CommandError::internal(error.to_string()))?;
            storage
                .set_setting(SERVER_CONFIGURATION_SETTING, &value)
                .await
                .map_err(|error| CommandError::internal(error.to_string()))?;
            storage
                .set_setting(
                    LEGACY_SERVER_CONFIGURATION_SETTING,
                    &serde_json::Value::Null,
                )
                .await
                .map_err(|error| CommandError::internal(error.to_string()))?;
            log.info(
                "configuration",
                "Legacy token configuration was removed; explicit pairing is required.",
            );
        }
        let stored_connection = storage
            .setting(CONNECTION_SETTING)
            .await
            .map_err(|error| CommandError::internal(error.to_string()))?
            .and_then(|value| serde_json::from_value::<OpenPrinterConnection>(value).ok());
        let (connection, credential_error) = if let Some(connection) = stored_connection {
            if connection.server_url == server_configuration.server_url {
                match key_manager.exists(&connection.credential_ref).await {
                    Ok(true) => (Some(connection), None),
                    Ok(false) => (
                        None,
                        Some("The paired private key is missing; pair again.".to_owned()),
                    ),
                    Err(error) => (None, Some(sanitize(&error.to_string()))),
                }
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };
        let agent_id = connection
            .as_ref()
            .map(|connection| connection.agent_id.clone());
        let initial_state = if agent_id.is_some() {
            AgentState::Disconnected
        } else {
            AgentState::Unconfigured
        };
        let handle = AgentHandle::new(AgentSnapshot {
            state: initial_state,
            pending_jobs: startup_pending.len(),
            active_errors: credential_error.into_iter().collect(),
        });

        let virtual_spooler = Arc::new(PerPrinterVirtualSpooler::new(app.clone()));
        let catalog = Arc::new(
            PrinterCatalog::load(
                storage.clone(),
                Arc::clone(&virtual_spooler),
                product.features,
                Arc::clone(&log),
            )
            .await?,
        );
        let jobs = Arc::new(JobLedger::load(storage.clone()).await?);
        let mut pending_summaries = Vec::with_capacity(startup_pending.len());
        for stored in startup_pending {
            let printer_name = catalog.get(stored.printer_id.as_str()).await.map_or_else(
                |_| "Printer".to_owned(),
                |printer| printer.reference.display_name,
            );
            pending_summaries.push(JobSummary {
                id: stored.id.to_string(),
                printer_id: stored.printer_id.to_string(),
                printer_name,
                idempotency_key: stored.idempotency_key,
                state: DesktopJobState::Received,
                received_at: stored.received_at.to_string(),
                updated_at: stored.updated_at.to_string(),
                attempts: stored.retry_attempts,
                error: stored.error.map(|error| error.message),
            });
        }
        jobs.restore_pending(pending_summaries).await?;

        let mut spoolers = SpoolerRegistry::new();
        spoolers.register(Arc::new(RawTcpSpooler::default()));
        spoolers.register(Arc::new(SystemQueueSpooler::default()));
        let virtual_spooler_for_registry: Arc<PerPrinterVirtualSpooler> =
            Arc::clone(&virtual_spooler);
        spoolers.register(virtual_spooler_for_registry);

        let (outbound_sender, outbound_receiver) =
            mpsc::channel::<OutboundRequest>(OUTBOUND_CHANNEL_CAPACITY);
        let reporter = Arc::new(OutboundChannelReporter::new(outbound_sender));
        let agent = Arc::new(
            AgentBuilder::new()
                .repository(Arc::new(storage.clone()))
                .printer_resolver(catalog.resolver())
                .spooler_registry(Arc::new(spoolers))
                .outbound_reporter(reporter)
                .handle(handle)
                .build()
                .map_err(|error| CommandError::internal(error.to_string()))?,
        );
        let pairing_client = PairingClient::new(Duration::from_secs(20))
            .map_err(|error| CommandError::internal(error.to_string()))?;
        let (connection_control, control_receiver) = mpsc::channel(8);
        let service = Arc::new(Self {
            app,
            product,
            storage,
            key_manager,
            agent,
            catalog,
            jobs,
            log,
            started_at: Instant::now(),
            last_connection_at: RwLock::new(None),
            agent_id: RwLock::new(agent_id.clone()),
            server_configuration: RwLock::new(server_configuration),
            connection: RwLock::new(connection),
            connection_state: RwLock::new(if agent_id.is_some() {
                OpenPrinterConnectionState::Paired
            } else if migrated_legacy || has_stored_configuration {
                OpenPrinterConnectionState::Unpaired
            } else {
                OpenPrinterConnectionState::Idle
            }),
            connected_service: RwLock::new(None),
            shutdown: CancellationToken::new(),
            startup_recovered_submissions: recovery.recovered_submissions,
            paths,
            pairing_client,
            provider_operation: Mutex::new(()),
            provider_generation: AtomicU64::new(0),
            connection_control,
            virtual_spooler,
        });

        service.spawn_observers();
        tauri::async_runtime::spawn(run_connection_supervisor(
            Arc::clone(&service),
            outbound_receiver,
            control_receiver,
        ));
        let discovery_service = Arc::clone(&service);
        tauri::async_runtime::spawn(async move {
            if let Err(error) = discovery_service.refresh_printers().await {
                discovery_service.log.warn("discovery", error.to_string());
            }
        });
        Ok(service)
    }

    pub async fn status(&self, start_on_login: bool, version: String) -> AgentStatus {
        let snapshot = self.agent.handle().snapshot().await;
        let agent_id = self.agent_id.read().await.clone();
        AgentStatus {
            agent_id,
            product: ProductSummary::from(&self.product),
            last_connection_at: self.last_connection_at.read().await.clone(),
            version,
            pending_jobs: snapshot.pending_jobs,
            active_errors: snapshot.active_errors,
            start_on_login,
            dashboard_url: None,
            platform: platform_label(),
            server_configuration: self.server_configuration.read().await.clone(),
            connected_service: self.connected_service.read().await.clone(),
            connection_state: *self.connection_state.read().await,
        }
    }

    pub async fn discover_server(&self) -> Result<DiscoveredServiceSummary, CommandError> {
        *self.connection_state.write().await = OpenPrinterConnectionState::Discovering;
        self.emit(STATE_CHANGED_EVENT);
        let server_url = self.server_configuration.read().await.server_url.clone();
        match self.pairing_client.discover(&server_url).await {
            Ok(discovered) => {
                let phase = if self.connection.read().await.is_some() {
                    OpenPrinterConnectionState::Paired
                } else {
                    OpenPrinterConnectionState::Unpaired
                };
                self.set_connection_phase(phase).await;
                Ok(DiscoveredServiceSummary {
                    name: discovered.document.server.name,
                    server_id: discovered.document.server.id,
                    server_version: discovered.document.server.version,
                    pairing_url: discovered.pairing_url.to_string(),
                    gateway_url: discovered.gateway_url.to_string(),
                })
            }
            Err(error) => {
                *self.connection_state.write().await = OpenPrinterConnectionState::DiscoveryFailed;
                self.set_connection_error(error.to_string()).await;
                Err(CommandError::new("discovery_failed", error.to_string()))
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    pub async fn pair_server(
        &self,
        code: String,
        agent_name: String,
    ) -> Result<DiscoveredServiceSummary, CommandError> {
        if agent_name.trim() != agent_name || agent_name.is_empty() || agent_name.len() > 128 {
            return Err(CommandError::new(
                "invalid_agent_name",
                "Agent name must contain 1 to 128 characters without surrounding whitespace.",
            ));
        }
        let _provider_operation = self.provider_operation.lock().await;
        if self.connection.read().await.is_some() {
            return Err(CommandError::new(
                "already_paired",
                "Forget the current server before pairing again.",
            ));
        }
        self.agent
            .handle()
            .transition(AgentState::Pairing)
            .await
            .map_err(|error| CommandError::internal(error.to_string()))?;
        *self.connection_state.write().await = OpenPrinterConnectionState::Pairing;
        self.emit(STATE_CHANGED_EVENT);

        let server_url = self.server_configuration.read().await.server_url.clone();
        let discovered = match self.pairing_client.discover(&server_url).await {
            Ok(discovered) => discovered,
            Err(error) => {
                let _ = self
                    .agent
                    .handle()
                    .transition(AgentState::Unconfigured)
                    .await;
                *self.connection_state.write().await = OpenPrinterConnectionState::DiscoveryFailed;
                return Err(CommandError::new("discovery_failed", error.to_string()));
            }
        };
        let generated = match self
            .key_manager
            .generate(&discovered.document.server.id)
            .await
        {
            Ok(generated) => generated,
            Err(error) => {
                let _ = self
                    .agent
                    .handle()
                    .transition(AgentState::Unconfigured)
                    .await;
                *self.connection_state.write().await = OpenPrinterConnectionState::Unpaired;
                return Err(CommandError::new(
                    "credential_generation_failed",
                    error.to_string(),
                ));
            }
        };
        let request = oppa_protocol::PairingRequest {
            protocol_version: oppa_protocol::PROTOCOL_VERSION.to_owned(),
            code,
            agent: oppa_protocol::PairingAgent {
                name: agent_name,
                version: env!("CARGO_PKG_VERSION").to_owned(),
                platform: std::env::consts::OS.to_owned(),
                installation_id: format!("installation_{}", Uuid::new_v4()),
            },
            credential: oppa_protocol::PairingCredential {
                algorithm: oppa_protocol::SIGNATURE_ALGORITHM.to_owned(),
                public_key: generated.public_key,
            },
        };
        let paired = match self.pairing_client.pair(&discovered, &request).await {
            Ok(paired) => paired,
            Err(error) => {
                let _ = self.key_manager.delete(&generated.credential_ref).await;
                let _ = self
                    .agent
                    .handle()
                    .transition(AgentState::Unconfigured)
                    .await;
                *self.connection_state.write().await = OpenPrinterConnectionState::Unpaired;
                return Err(CommandError::new("pairing_failed", error.to_string()));
            }
        };
        let connection = OpenPrinterConnection {
            server_url: server_url.clone(),
            server_id: paired.server_id,
            agent_id: paired.agent_id,
            key_id: paired.key_id,
            credential_ref: generated.credential_ref,
        };
        let value = serde_json::to_value(&connection)
            .map_err(|error| CommandError::internal(error.to_string()))?;
        if let Err(error) = self.storage.set_setting(CONNECTION_SETTING, &value).await {
            let cleanup = self.key_manager.delete(&connection.credential_ref).await;
            let _ = self
                .agent
                .handle()
                .transition(AgentState::Unconfigured)
                .await;
            *self.connection_state.write().await = OpenPrinterConnectionState::Unpaired;
            let detail = if cleanup.is_ok() {
                "The newly generated local key was deleted safely."
            } else {
                "Local key cleanup also failed and requires credential-store attention."
            };
            return Err(CommandError::new(
                "connection_store_failed",
                format!("Paired credential metadata could not be saved: {error}. {detail}"),
            ));
        }
        *self.agent_id.write().await = Some(connection.agent_id.clone());
        *self.connection.write().await = Some(connection);
        *self.connection_state.write().await = OpenPrinterConnectionState::Paired;
        self.agent.handle().set_active_errors(Vec::new()).await;
        self.agent
            .handle()
            .transition(AgentState::Disconnected)
            .await
            .map_err(|error| CommandError::internal(error.to_string()))?;
        let _ = self
            .connection_control
            .send(ConnectionControl::Reconnect)
            .await;
        self.save_recent_server(
            server_url.as_str(),
            Some(discovered.document.server.name.clone()),
        )
        .await;
        self.log
            .info("authentication", "OpenPrinter pairing completed.");
        self.emit(STATE_CHANGED_EVENT);
        Ok(DiscoveredServiceSummary {
            name: discovered.document.server.name,
            server_id: discovered.document.server.id,
            server_version: discovered.document.server.version,
            pairing_url: discovered.pairing_url.to_string(),
            gateway_url: discovered.gateway_url.to_string(),
        })
    }

    pub async fn list_printers(&self) -> Result<Vec<PrinterSummary>, CommandError> {
        self.catalog.list().await
    }

    pub async fn refresh_printers(&self) -> Result<Vec<PrinterSummary>, CommandError> {
        let printers = self.catalog.refresh().await?;
        self.emit(PRINTERS_CHANGED_EVENT);
        self.publish_inventory();
        Ok(printers)
    }

    pub async fn configure_printer(
        &self,
        printer_id: &str,
        changes: crate::models::ConfigurePrinterChanges,
    ) -> Result<PrinterSummary, CommandError> {
        let printer = self.catalog.configure(printer_id, changes).await?;
        self.emit(PRINTERS_CHANGED_EVENT);
        self.publish_inventory();
        Ok(printer)
    }

    pub async fn add_manual_printer(
        &self,
        input: ManualPrinterInput,
    ) -> Result<PrinterSummary, CommandError> {
        let printer = self.catalog.add_manual(input).await?;
        self.log.info("printer", "Manual network printer added.");
        self.emit(PRINTERS_CHANGED_EVENT);
        self.publish_inventory();
        Ok(printer)
    }

    pub async fn create_virtual_printer(
        &self,
        input: VirtualPrinterInput,
    ) -> Result<PrinterSummary, CommandError> {
        let printer = self.catalog.create_virtual(input).await?;
        self.log.info("printer", "Virtual printer created.");
        self.emit(PRINTERS_CHANGED_EVENT);
        self.publish_inventory();
        Ok(printer)
    }

    pub async fn update_virtual_printer(
        &self,
        printer_id: &str,
        mode: VirtualPrinterMode,
        delay_ms: u64,
    ) -> Result<PrinterSummary, CommandError> {
        let printer = self
            .catalog
            .update_virtual(printer_id, mode, delay_ms)
            .await?;
        self.emit(PRINTERS_CHANGED_EVENT);
        self.publish_inventory();
        Ok(printer)
    }

    pub async fn clear_virtual_history(&self, printer_id: &str) -> Result<(), CommandError> {
        self.catalog.clear_virtual_history(printer_id).await?;
        self.emit(PRINTERS_CHANGED_EVENT);
        Ok(())
    }

    pub async fn set_virtual_printer_sound(
        &self,
        printer_id: &str,
        enabled: bool,
    ) -> Result<(), CommandError> {
        let id =
            PrinterId::new(printer_id).map_err(|_| CommandError::invalid("Invalid printer ID."))?;
        self.virtual_spooler
            .set_sound(&id, enabled)
            .await
            .map_err(|error| CommandError::new("printer_not_found", error.to_string()))
    }

    pub async fn remove_printer(&self, printer_id: &str) -> Result<(), CommandError> {
        self.catalog.remove(printer_id).await?;
        self.log.info("printer", "Printer removed.");
        self.emit(PRINTERS_CHANGED_EVENT);
        self.publish_inventory();
        Ok(())
    }

    pub async fn send_test_print(&self, printer_id: &str) -> Result<JobSummary, CommandError> {
        let printer = self.catalog.get(printer_id).await?;
        if !self.catalog.is_active(&printer) {
            return Err(CommandError::new(
                "feature_unavailable",
                "This printer connection is disabled in the current product build.",
            ));
        }
        if !printer.reference.enabled {
            return Err(CommandError::invalid("Printer is disabled."));
        }
        let width = printer
            .virtual_width
            .or_else(|| {
                printer
                    .capabilities
                    .as_ref()
                    .and_then(|capabilities| capabilities.receipt_widths_mm.first().copied())
            })
            .unwrap_or(80);
        let width = match width {
            58 => ReceiptWidth::Mm58,
            _ => ReceiptWidth::Mm80,
        };
        let job_id = format!("job_test_{}", Uuid::new_v4());
        let idempotency_key = format!("test_{}", Uuid::new_v4());
        let now = Timestamp::now().to_string();
        let summary = JobSummary {
            id: job_id.clone(),
            printer_id: printer.reference.id.to_string(),
            printer_name: printer.reference.display_name.clone(),
            idempotency_key: idempotency_key.clone(),
            state: DesktopJobState::Received,
            received_at: now.clone(),
            updated_at: now.clone(),
            attempts: 0,
            error: None,
        };
        self.jobs.insert(summary).await?;
        self.emit(JOBS_CHANGED_EVENT);

        let job = PrintJob {
            job_id: job_id.clone(),
            idempotency_key,
            printer_id: printer.reference.id.to_string(),
            created_at: now,
            document: test_document(width, &self.product.product_name),
            metadata: None,
        };
        match self.agent.submit_local_test(&job).await {
            Ok(_) => {
                self.jobs
                    .update(
                        &job_id,
                        DesktopJobState::Submitted,
                        None,
                        Some(1),
                        Timestamp::now().to_string(),
                    )
                    .await?;
            }
            Err(error) => {
                let message = sanitize(&error.to_string());
                self.jobs
                    .update(
                        &job_id,
                        DesktopJobState::Failed,
                        Some(message.clone()),
                        Some(1),
                        Timestamp::now().to_string(),
                    )
                    .await?;
                self.log.warn("job", &message);
                self.emit(JOBS_CHANGED_EVENT);
                self.emit(PRINTERS_CHANGED_EVENT);
                return Err(CommandError::new("test_print_failed", message));
            }
        }
        self.log
            .info("job", format!("Local test job {job_id} completed."));
        self.emit(JOBS_CHANGED_EVENT);
        self.emit(PRINTERS_CHANGED_EVENT);
        self.jobs
            .list()
            .await
            .into_iter()
            .find(|job| job.id == job_id)
            .ok_or_else(|| CommandError::internal("Test job disappeared from local history."))
    }

    pub async fn list_recent_jobs(&self) -> Vec<JobSummary> {
        self.jobs.list().await
    }

    pub async fn clear_jobs(&self) -> Result<(), CommandError> {
        self.jobs.clear().await
    }

    pub fn clear_logs(&self) {
        self.log.clear();
    }

    pub async fn diagnostics(&self) -> Diagnostics {
        let migration = self.storage.migration_version().await;
        if let Err(error) = &migration {
            self.log.error("storage", error.to_string());
        }
        Diagnostics {
            agent_version: env!("CARGO_PKG_VERSION").to_owned(),
            product_id: self.product.product_id.to_string(),
            platform: platform_label(),
            connection_state: *self.connection_state.read().await,
            database_healthy: migration.is_ok(),
            migration_version: migration.unwrap_or(0),
            discovery_providers: self.catalog.discovery_status().await,
            logs: self.log.entries(),
        }
    }

    pub async fn export_diagnostics(&self) -> Result<String, CommandError> {
        let mut printers = self.catalog.list().await?;
        for printer in &mut printers {
            if printer.is_virtual {
                printer.history = Some(Vec::new());
            }
        }
        let export = DiagnosticExport {
            generated_at: Utc::now().to_rfc3339(),
            diagnostics: self.diagnostics().await,
            printers,
            recent_jobs: self.jobs.list().await,
            feature_availability: FeatureAvailability {
                virtual_printer: self.product.features.virtual_printer,
                system_printer_discovery: self.product.features.system_printer_discovery,
                network_printer_discovery: self.product.features.network_printer_discovery,
                usb_printer_discovery: self.product.features.usb_printer_discovery,
                remote_diagnostics: self.product.features.remote_diagnostics,
            },
        };
        let bytes = serde_json::to_vec_pretty(&export)
            .map_err(|error| CommandError::internal(error.to_string()))?;
        let filename = format!(
            "{}-diagnostics-{}.json",
            self.product.product_id,
            Utc::now().format("%Y%m%dT%H%M%SZ")
        );
        let path = self.paths.log_dir.join(filename);
        tokio::fs::write(&path, bytes).await.map_err(|error| {
            CommandError::internal(format!("could not write diagnostics: {error}"))
        })?;
        self.log
            .info("diagnostics", "Sanitized diagnostics bundle exported.");
        Ok(path.to_string_lossy().into_owned())
    }

    pub async fn reconnect(&self) -> Result<(), CommandError> {
        if self.agent_id.read().await.is_none() {
            return Err(CommandError::new(
                "not_configured",
                "Pair this agent before connecting.",
            ));
        }
        self.connection_control
            .send(ConnectionControl::Reconnect)
            .await
            .map_err(|_| CommandError::internal("Connection supervisor is unavailable."))?;
        self.log.info("transport", "Reconnect requested.");
        Ok(())
    }

    pub async fn set_server_configuration(
        &self,
        input: OpenPrinterServerConfigurationInput,
    ) -> Result<OpenPrinterServerConfiguration, CommandError> {
        let configuration = OpenPrinterServerConfiguration::from_input(&input)?;
        self.apply_server_configuration(configuration).await
    }

    pub async fn reset_server_configuration(
        &self,
    ) -> Result<OpenPrinterServerConfiguration, CommandError> {
        let configuration = OpenPrinterServerConfiguration::default();
        self.apply_server_configuration(configuration).await
    }

    pub async fn forget_server(&self) -> Result<(), CommandError> {
        let _provider_operation = self.provider_operation.lock().await;
        self.forget_connection().await?;
        self.provider_generation.fetch_add(1, Ordering::AcqRel);
        let _ = self
            .connection_control
            .send(ConnectionControl::ConfigurationChanged)
            .await;
        self.log
            .info("authentication", "Local paired credential was deleted.");
        self.emit(STATE_CHANGED_EVENT);
        Ok(())
    }

    /// Returns servers that were paired in this installation, most-recent first.
    pub async fn list_recent_servers(&self) -> Result<Vec<RecentServer>, CommandError> {
        let servers: Vec<RecentServer> = self
            .storage
            .setting(RECENT_SERVERS_SETTING)
            .await
            .map_err(|error| CommandError::internal(error.to_string()))?
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        Ok(servers)
    }

    /// Forgets the current connection and sets a different server URL, ready for pairing.
    pub async fn apply_recent_server(&self, server_url: String) -> Result<(), CommandError> {
        let configuration =
            OpenPrinterServerConfiguration::from_input(&OpenPrinterServerConfigurationInput {
                server_url,
            })?;
        self.apply_server_configuration(configuration).await?;
        Ok(())
    }

    /// Removes a server URL from the persisted recent-server list.
    pub async fn delete_recent_server(&self, server_url: String) -> Result<(), CommandError> {
        let mut servers: Vec<RecentServer> = self
            .storage
            .setting(RECENT_SERVERS_SETTING)
            .await
            .map_err(|error| CommandError::internal(error.to_string()))?
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        let initial_len = servers.len();
        servers.retain(|server| server.server_url != server_url);

        if servers.len() == initial_len {
            return Ok(());
        }

        let value = serde_json::to_value(&servers)
            .map_err(|error| CommandError::internal(error.to_string()))?;
        self.storage
            .set_setting(RECENT_SERVERS_SETTING, &value)
            .await
            .map_err(|error| CommandError::internal(error.to_string()))?;

        Ok(())
    }

    /// Prepends `server_url` to the persisted recent-server list (best-effort, silently ignored on failure).
    async fn save_recent_server(&self, server_url: &str, name: Option<String>) {
        use chrono::Utc;

        let mut servers: Vec<RecentServer> = self
            .storage
            .setting(RECENT_SERVERS_SETTING)
            .await
            .ok()
            .flatten()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        servers.retain(|s| s.server_url != server_url);
        servers.insert(
            0,
            RecentServer {
                server_url: server_url.to_owned(),
                name,
                paired_at: Utc::now().to_rfc3339(),
            },
        );
        servers.truncate(10);

        if let Ok(value) = serde_json::to_value(&servers) {
            let _ = self
                .storage
                .set_setting(RECENT_SERVERS_SETTING, &value)
                .await;
        }
    }

    async fn apply_server_configuration(
        &self,
        configuration: OpenPrinterServerConfiguration,
    ) -> Result<OpenPrinterServerConfiguration, CommandError> {
        let _provider_operation = self.provider_operation.lock().await;
        if *self.server_configuration.read().await == configuration {
            return Ok(configuration);
        }

        self.forget_connection().await?;
        let value = serde_json::to_value(&configuration)
            .map_err(|error| CommandError::internal(error.to_string()))?;
        if let Err(error) = self
            .storage
            .set_setting(SERVER_CONFIGURATION_SETTING, &value)
            .await
        {
            let diagnostic = sanitize(&error.to_string());
            self.log.error(
                "configuration",
                format!(
                    "Could not persist the OpenPrinter server URL after deleting the prior credential: {diagnostic}"
                ),
            );
            self.provider_generation.fetch_add(1, Ordering::AcqRel);
            self.force_unconfigured(
                "The server URL was not saved. Pair again after choosing the intended server.",
            )
            .await;
            let _ = self
                .connection_control
                .send(ConnectionControl::ConfigurationChanged)
                .await;
            self.emit(STATE_CHANGED_EVENT);
            return Err(CommandError::new(
                "configuration_store_failed",
                "The server URL could not be saved. The prior credential was deleted to keep the connection safe.",
            ));
        }

        *self.server_configuration.write().await = configuration.clone();
        self.provider_generation.fetch_add(1, Ordering::AcqRel);
        self.force_unconfigured_with_errors(Vec::new()).await;
        self.connection_control
            .send(ConnectionControl::ConfigurationChanged)
            .await
            .map_err(|_| CommandError::internal("Connection supervisor is unavailable."))?;
        self.log.info(
            "configuration",
            "OpenPrinter server URL changed; prior credentials were deleted.",
        );
        self.emit(STATE_CHANGED_EVENT);
        Ok(configuration)
    }

    async fn forget_connection(&self) -> Result<(), CommandError> {
        if let Some(connection) = self.connection.write().await.take() {
            self.key_manager
                .delete(&connection.credential_ref)
                .await
                .map_err(|error| {
                    CommandError::new("credential_delete_failed", error.to_string())
                })?;
        }
        self.storage
            .set_setting(CONNECTION_SETTING, &serde_json::Value::Null)
            .await
            .map_err(|error| CommandError::new("connection_store_failed", error.to_string()))?;
        *self.connection_state.write().await = OpenPrinterConnectionState::Unpaired;
        self.force_unconfigured_with_errors(Vec::new()).await;
        Ok(())
    }

    async fn force_unconfigured(&self, message: impl AsRef<str>) {
        self.force_unconfigured_with_errors(vec![sanitize(message.as_ref())])
            .await;
    }

    async fn force_unconfigured_with_errors(&self, errors: Vec<String>) {
        *self.agent_id.write().await = None;
        *self.connected_service.write().await = None;

        let current = self.agent.handle().snapshot().await.state;
        if matches!(
            current,
            AgentState::Connecting | AgentState::Connected | AgentState::Degraded
        ) {
            let _ = self
                .agent
                .handle()
                .transition(AgentState::Disconnected)
                .await;
        }
        if self.agent.handle().snapshot().await.state != AgentState::Unconfigured {
            let _ = self
                .agent
                .handle()
                .transition(AgentState::Unconfigured)
                .await;
        }
        self.agent.handle().set_active_errors(errors).await;
    }

    pub fn open_product_link(&self, link: ProductLink) -> Result<(), CommandError> {
        let url = match link {
            ProductLink::Documentation => &self.product.branding.documentation_url,
            ProductLink::Support => &self.product.branding.support_url,
            ProductLink::Privacy => self.product.legal.privacy_url.as_ref().ok_or_else(|| {
                CommandError::new(
                    "link_unavailable",
                    "This product does not define a privacy policy URL.",
                )
            })?,
            ProductLink::Terms => self.product.legal.terms_url.as_ref().ok_or_else(|| {
                CommandError::new(
                    "link_unavailable",
                    "This product does not define a terms of service URL.",
                )
            })?,
        };
        SystemBrowser
            .open(url)
            .map_err(|error| CommandError::new("browser_failed", error.to_string()))
    }

    pub async fn shutdown(&self) {
        self.shutdown.cancel();
        let _ = self
            .connection_control
            .send(ConnectionControl::Shutdown)
            .await;
        let _ = self
            .agent
            .handle()
            .transition(AgentState::ShuttingDown)
            .await;
        self.emit(STATE_CHANGED_EVENT);
    }

    pub(crate) fn emit(&self, event: &str) {
        let _ = self.app.emit(event, ());
    }

    pub(crate) async fn set_connection_error(&self, message: impl AsRef<str>) {
        let message = sanitize(message.as_ref());
        self.agent
            .handle()
            .set_active_errors(vec![message.clone()])
            .await;
        self.log.warn("transport", message);
        self.emit(STATE_CHANGED_EVENT);
    }

    pub(crate) async fn authentication_failed(&self, message: impl AsRef<str>, revoked: bool) {
        let message = sanitize(message.as_ref());
        let current = self.agent.handle().snapshot().await.state;
        if matches!(
            current,
            AgentState::Connecting | AgentState::Connected | AgentState::Degraded
        ) {
            let _ = self
                .agent
                .handle()
                .transition(AgentState::Disconnected)
                .await;
        }
        *self.connection_state.write().await = if revoked {
            OpenPrinterConnectionState::CredentialRevoked
        } else {
            OpenPrinterConnectionState::AuthenticationFailed
        };
        self.agent
            .handle()
            .set_active_errors(vec![message.clone()])
            .await;
        self.log.error("authentication", message);
        self.emit(STATE_CHANGED_EVENT);
    }

    pub(crate) async fn transition(&self, state: AgentState) {
        if let Err(error) = self.agent.handle().transition(state).await {
            self.log.warn("agent", error.to_string());
        }
        self.emit(STATE_CHANGED_EVENT);
    }

    pub(crate) async fn set_connection_phase(&self, state: OpenPrinterConnectionState) {
        *self.connection_state.write().await = state;
        self.emit(STATE_CHANGED_EVENT);
    }

    pub(crate) fn uptime_seconds(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    fn publish_inventory(&self) {
        let _ = self
            .connection_control
            .try_send(ConnectionControl::PublishInventory);
    }

    fn spawn_observers(self: &Arc<Self>) {
        let state_service = Arc::clone(self);
        let mut state_updates = self.agent.handle().subscribe();
        let state_shutdown = self.shutdown.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::select! {
                    () = state_shutdown.cancelled() => break,
                    changed = state_updates.changed() => {
                        if changed.is_err() {
                            break;
                        }
                        state_service.emit(STATE_CHANGED_EVENT);
                    }
                }
            }
        });

        let job_service = Arc::clone(self);
        let mut job_updates = self.agent.handle().subscribe_job_events();
        let job_shutdown = self.shutdown.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::select! {
                    () = job_shutdown.cancelled() => break,
                    event = job_updates.recv() => {
                        match event {
                            Ok(event) => {
                                if let Err(error) = job_service.jobs.apply_event(&event).await {
                                    job_service.log.warn("job", error.to_string());
                                }
                                job_service.emit(JOBS_CHANGED_EVENT);
                                if matches!(
                                    event,
                                    oppa_agent::AgentEvent::JobSubmitted { .. }
                                        | oppa_agent::AgentEvent::JobFailed { .. }
                                        | oppa_agent::AgentEvent::JobCancelled { .. }
                                ) {
                                    job_service.emit(PRINTERS_CHANGED_EVENT);
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                                job_service.log.warn(
                                    "job",
                                    format!("Desktop job observer skipped {skipped} event(s)."),
                                );
                                job_service.emit(JOBS_CHANGED_EVENT);
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                }
            }
        });
    }

    pub(crate) async fn apply_process_outcome(
        &self,
        outcome: &ProcessOutcome,
    ) -> Result<(), CommandError> {
        let (state, error) = match outcome {
            ProcessOutcome::Submitted { .. } => (DesktopJobState::Submitted, None),
            ProcessOutcome::Failed { message, .. } => {
                let error = message.as_ref().and_then(|message| {
                    let AgentMessageKind::JobFailed(failed) = &message.kind else {
                        return None;
                    };
                    Some(failed.error.message.clone())
                });
                (DesktopJobState::Failed, error)
            }
            ProcessOutcome::Cancelled { .. } => (DesktopJobState::Cancelled, None),
            ProcessOutcome::Skipped { state, .. } => {
                let state = match state {
                    oppa_core::JobState::Received | oppa_core::JobState::Submitting => {
                        DesktopJobState::Received
                    }
                    oppa_core::JobState::Submitted => DesktopJobState::Submitted,
                    oppa_core::JobState::Failed => DesktopJobState::Failed,
                    oppa_core::JobState::Cancelled => DesktopJobState::Cancelled,
                };
                (state, None)
            }
        };
        self.jobs
            .update(
                outcome.job_id().as_str(),
                state,
                error,
                Some(1),
                Timestamp::now().to_string(),
            )
            .await
    }
}

fn resolve_paths(product: &ProductConfig) -> Result<AppPaths, CommandError> {
    let segments = product.application_id.split('.').collect::<Vec<_>>();
    let qualifier = segments.first().copied().unwrap_or("com");
    let organization = segments.get(1).copied().unwrap_or("neplextech");
    let application = segments.last().copied().unwrap_or("oppa");
    resolve_app_paths(qualifier, organization, application)
        .map_err(|error| CommandError::internal(error.to_string()))
}

fn platform_label() -> String {
    platform_info().map_or_else(
        |_| format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        |platform| {
            format!(
                "{} {}",
                display_operating_system(platform.operating_system),
                platform.architecture
            )
        },
    )
}

fn display_operating_system(value: &str) -> &str {
    match value {
        "macos" => "macOS",
        "windows" => "Windows",
        "linux" => "Linux",
        other => other,
    }
}

fn test_document(width: ReceiptWidth, product_name: &str) -> PrintDocument {
    PrintDocument {
        width,
        sections: vec![
            PrintSection::Text {
                value: format!("{product_name} TEST PRINT"),
                align: Some(TextAlignment::Center),
                bold: Some(true),
            },
            PrintSection::Divider,
            PrintSection::Text {
                value: "Connection verified".to_owned(),
                align: Some(TextAlignment::Center),
                bold: None,
            },
            PrintSection::Text {
                value: "Submitted by the local desktop agent.".to_owned(),
                align: None,
                bold: None,
            },
            PrintSection::Feed { lines: 3 },
            PrintSection::Cut,
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::{display_operating_system, test_document};
    use oppa_protocol::{ReceiptWidth, Validate};

    #[test]
    fn local_test_document_is_protocol_valid() {
        assert!(
            test_document(ReceiptWidth::Mm58, "Branded Agent")
                .validate()
                .is_ok()
        );
        assert!(
            test_document(ReceiptWidth::Mm80, "Branded Agent")
                .validate()
                .is_ok()
        );
    }

    #[test]
    fn platform_labels_are_user_facing() {
        assert_eq!(display_operating_system("macos"), "macOS");
        assert_eq!(display_operating_system("windows"), "Windows");
    }
}
