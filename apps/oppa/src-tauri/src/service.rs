use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use chrono::{Duration as ChronoDuration, Utc};
use oppa_agent::{Agent, AgentBuilder, AgentHandle, AgentSnapshot, AgentState, ProcessOutcome};
use oppa_auth::{
    AuthorizationClient, AuthorizationEndpoints, CredentialManager, DEFAULT_CALLBACK_TIMEOUT,
};
use oppa_core::Timestamp;
use oppa_platform::{
    AppPaths, BrowserOpener, KeyringCredentialStore, SystemBrowser, platform_info,
    resolve_app_paths,
};
use oppa_product::{ProductConfig, embedded_product};
use oppa_protocol::{
    AgentMessageKind, PrintDocument, PrintJob, PrintSection, ReceiptWidth, TextAlignment,
};
use oppa_spooler::{RawTcpSpooler, SpoolerRegistry, SystemQueueSpooler};
use oppa_storage::{DEFAULT_MAX_PENDING_JOBS, JobRepository, SqliteStorage};
use tauri::{AppHandle, Emitter};
use tokio::sync::{RwLock, mpsc};
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
        AgentStatus, AuthorizationStart, DesktopJobState, DiagnosticExport, Diagnostics,
        FeatureAvailability, GatewayState, JobSummary, ManualPrinterInput, PrinterSummary,
        ProductLink, ProductSummary, VirtualPrinterInput, VirtualPrinterMode,
    },
    printer_catalog::PrinterCatalog,
    virtual_spooler::PerPrinterVirtualSpooler,
};

pub const STATE_CHANGED_EVENT: &str = "oppa://state-changed";
pub const PRINTERS_CHANGED_EVENT: &str = "oppa://printers-changed";
pub const JOBS_CHANGED_EVENT: &str = "oppa://jobs-changed";

const OUTBOUND_CHANNEL_CAPACITY: usize = 256;

/// Long-lived desktop host for the shell-independent agent runtime.
pub struct DesktopService {
    pub(crate) app: AppHandle,
    pub(crate) product: ProductConfig,
    pub(crate) storage: SqliteStorage,
    pub(crate) credentials: CredentialManager,
    pub(crate) agent: Arc<Agent>,
    pub(crate) catalog: Arc<PrinterCatalog>,
    pub(crate) jobs: Arc<JobLedger>,
    pub(crate) log: Arc<DiagnosticLog>,
    pub(crate) started_at: Instant,
    pub(crate) last_connection_at: RwLock<Option<String>>,
    pub(crate) agent_id: RwLock<Option<String>>,
    pub(crate) shutdown: CancellationToken,
    pub(crate) startup_recovered_submissions: usize,
    paths: AppPaths,
    pub(crate) authorization: AuthorizationClient,
    authorization_active: AtomicBool,
    connection_control: mpsc::Sender<ConnectionControl>,
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

        let credential_store = Arc::new(
            KeyringCredentialStore::new(product.application_id.clone())
                .map_err(|error| CommandError::internal(error.to_string()))?,
        );
        let credentials = CredentialManager::new(credential_store);
        let (agent_id, credential_error) = match credentials.load().await {
            Ok(Some(tokens)) => (Some(tokens.agent_id.to_string()), None),
            Ok(None) => (None, None),
            Err(error) => {
                let message = sanitize(&error.to_string());
                log.error("authentication", &message);
                (None, Some(message))
            }
        };
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

        let virtual_spooler = Arc::new(PerPrinterVirtualSpooler::default());
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
        spoolers.register(virtual_spooler);

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
        let authorization = AuthorizationClient::new(
            AuthorizationEndpoints::from_product(&product),
            product.protocol.client_id.clone(),
            vec!["openprinter.agent".to_owned()],
            Duration::from_secs(20),
        )
        .map_err(|error| CommandError::internal(error.to_string()))?;
        let (connection_control, control_receiver) = mpsc::channel(8);
        let service = Arc::new(Self {
            app,
            product,
            storage,
            credentials,
            agent,
            catalog,
            jobs,
            log,
            started_at: Instant::now(),
            last_connection_at: RwLock::new(None),
            agent_id: RwLock::new(agent_id),
            shutdown: CancellationToken::new(),
            startup_recovered_submissions: recovery.recovered_submissions,
            paths,
            authorization,
            authorization_active: AtomicBool::new(false),
            connection_control,
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

    pub async fn status(&self, start_on_login: bool) -> AgentStatus {
        let snapshot = self.agent.handle().snapshot().await;
        let agent_id = self.agent_id.read().await.clone();
        AgentStatus {
            configured: agent_id.is_some(),
            agent_id,
            product: ProductSummary::from(&self.product),
            state: snapshot.state,
            gateway_state: match snapshot.state {
                AgentState::Connecting => GatewayState::Connecting,
                AgentState::Connected | AgentState::Degraded => GatewayState::Online,
                AgentState::Unconfigured
                | AgentState::Authorizing
                | AgentState::Disconnected
                | AgentState::ShuttingDown => GatewayState::Offline,
            },
            last_connection_at: self.last_connection_at.read().await.clone(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            pending_jobs: snapshot.pending_jobs,
            active_errors: snapshot.active_errors,
            start_on_login,
            dashboard_url: None,
            platform: platform_label(),
        }
    }

    pub async fn begin_authorization(self: &Arc<Self>) -> Result<AuthorizationStart, CommandError> {
        if self
            .authorization_active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(CommandError::new(
                "authorization_in_progress",
                "An authorization flow is already in progress.",
            ));
        }
        let snapshot = self.agent.handle().snapshot().await;
        if snapshot.state != AgentState::Unconfigured {
            self.authorization_active.store(false, Ordering::Release);
            return Err(CommandError::new(
                "already_configured",
                "This agent is already configured.",
            ));
        }
        self.agent
            .handle()
            .transition(AgentState::Authorizing)
            .await
            .map_err(|error| {
                self.authorization_active.store(false, Ordering::Release);
                CommandError::internal(error.to_string())
            })?;

        let pending = match self.authorization.begin().await {
            Ok(pending) => pending,
            Err(error) => {
                self.authorization_active.store(false, Ordering::Release);
                let _ = self
                    .agent
                    .handle()
                    .transition(AgentState::Unconfigured)
                    .await;
                return Err(CommandError::new("authorization_failed", error.to_string()));
            }
        };
        if let Err(error) = pending.open_system_browser() {
            self.authorization_active.store(false, Ordering::Release);
            let _ = self
                .agent
                .handle()
                .transition(AgentState::Unconfigured)
                .await;
            return Err(CommandError::new("browser_failed", error.to_string()));
        }
        let authorization_url = pending.authorization_url.to_string();
        let expires_at = (Utc::now()
            + ChronoDuration::from_std(DEFAULT_CALLBACK_TIMEOUT)
                .unwrap_or_else(|_| ChronoDuration::minutes(5)))
        .to_rfc3339();
        self.log
            .info("authentication", "Browser authorization started.");
        let service = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            match pending.complete(&service.authorization).await {
                Ok(tokens) => {
                    let agent_id = tokens.agent_id.to_string();
                    match service.credentials.save(&tokens).await {
                        Ok(()) => {
                            *service.agent_id.write().await = Some(agent_id);
                            service.agent.handle().set_active_errors(Vec::new()).await;
                            let _ = service
                                .agent
                                .handle()
                                .transition(AgentState::Disconnected)
                                .await;
                            service
                                .log
                                .info("authentication", "Authorization completed.");
                            let _ = service
                                .connection_control
                                .send(ConnectionControl::Reconnect)
                                .await;
                        }
                        Err(error) => {
                            service
                                .authorization_failed(format!(
                                    "Could not save credentials: {error}"
                                ))
                                .await;
                        }
                    }
                }
                Err(error) => {
                    service.authorization_failed(error.to_string()).await;
                }
            }
            service.authorization_active.store(false, Ordering::Release);
            service.emit(STATE_CHANGED_EVENT);
        });
        self.emit(STATE_CHANGED_EVENT);
        Ok(AuthorizationStart {
            authorization_url,
            expires_at,
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

    pub async fn diagnostics(&self) -> Diagnostics {
        let snapshot = self.agent.handle().snapshot().await;
        let migration = self.storage.migration_version().await;
        if let Err(error) = &migration {
            self.log.error("storage", error.to_string());
        }
        Diagnostics {
            agent_version: env!("CARGO_PKG_VERSION").to_owned(),
            product_id: self.product.product_id.to_string(),
            platform: platform_label(),
            connection_state: snapshot.state,
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
                "Authorize this agent before connecting.",
            ));
        }
        self.connection_control
            .send(ConnectionControl::Reconnect)
            .await
            .map_err(|_| CommandError::internal("Connection supervisor is unavailable."))?;
        self.log.info("transport", "Reconnect requested.");
        Ok(())
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

    pub(crate) async fn invalidate_credentials(&self, message: impl AsRef<str>) {
        let message = sanitize(message.as_ref());
        if let Err(error) = self.credentials.clear().await {
            self.log.error(
                "authentication",
                format!("Could not clear rejected credentials: {error}"),
            );
        }
        *self.agent_id.write().await = None;
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
        let _ = self
            .agent
            .handle()
            .transition(AgentState::Unconfigured)
            .await;
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

    async fn authorization_failed(&self, message: impl AsRef<str>) {
        let message = sanitize(message.as_ref());
        self.log.error("authentication", &message);
        self.agent.handle().set_active_errors(vec![message]).await;
        let _ = self
            .agent
            .handle()
            .transition(AgentState::Unconfigured)
            .await;
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
