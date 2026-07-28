use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use oppa_agent::{
    Agent, AgentBuilder, AgentEvent, AgentRuntimeError, CancelOutcome, OutboundReportError,
    OutboundReporter, ProcessOutcome, ReceiveJobOutcome, StaticPrinterResolver,
};
use oppa_core::{JobState, PrintJobId, PrinterId};
use oppa_printer::{PrinterConnection, PrinterRef};
use oppa_protocol::{
    AgentMessage, AgentMessageKind, PrintDocument, PrintJob, PrintSection, ProtocolVersion,
    ReceiptWidth, ServerMessage, ServerMessageKind,
};
use oppa_spooler::{SpoolerRegistry, VirtualSimulation, VirtualSpooler};
use oppa_storage::{
    DEFAULT_MAX_PENDING_JOBS, DEFAULT_OUTBOUND_STATUS_BATCH_SIZE, JobRepository, SqliteStorage,
};
use tokio::sync::Mutex;
use tokio::time::timeout;

const PRINTER_ID: &str = "printer_virtual_1";
const JOB_ID: &str = "job_1";
const IDEMPOTENCY_KEY: &str = "delivery_1";

#[derive(Clone)]
struct RecordingReporter {
    storage: Arc<SqliteStorage>,
    messages: Arc<Mutex<Vec<AgentMessage>>>,
}

impl RecordingReporter {
    fn new(storage: Arc<SqliteStorage>) -> Self {
        Self {
            storage,
            messages: Arc::new(Mutex::new(Vec::new())),
        }
    }

    async fn messages(&self) -> Vec<AgentMessage> {
        self.messages.lock().await.clone()
    }

    async fn message_types(&self) -> Vec<&'static str> {
        self.messages
            .lock()
            .await
            .iter()
            .map(AgentMessage::message_type)
            .collect()
    }
}

#[async_trait]
impl OutboundReporter for RecordingReporter {
    async fn report(&self, message: &AgentMessage) -> Result<(), OutboundReportError> {
        let (job_id, idempotency_key) = match &message.kind {
            AgentMessageKind::JobReceived(payload) => (&payload.job_id, &payload.idempotency_key),
            AgentMessageKind::JobSubmitted(payload) => {
                if !self
                    .storage
                    .has_completed_idempotency_key(&payload.idempotency_key)
                    .await
                    .map_err(|error| OutboundReportError::new(error.to_string()))?
                {
                    return Err(OutboundReportError::new(
                        "submitted report preceded its terminal commit",
                    ));
                }
                (&payload.job_id, &payload.idempotency_key)
            }
            AgentMessageKind::JobFailed(payload) => {
                if !self
                    .storage
                    .has_completed_idempotency_key(&payload.idempotency_key)
                    .await
                    .map_err(|error| OutboundReportError::new(error.to_string()))?
                {
                    return Err(OutboundReportError::new(
                        "failed report preceded its terminal commit",
                    ));
                }
                (&payload.job_id, &payload.idempotency_key)
            }
            _ => {
                return Err(OutboundReportError::new(
                    "job test reporter received a non-job message",
                ));
            }
        };

        if matches!(&message.kind, AgentMessageKind::JobReceived(_)) {
            let job_id = PrintJobId::new(job_id.clone())
                .map_err(|error| OutboundReportError::new(error.to_string()))?;
            let is_pending = self
                .storage
                .pending()
                .await
                .map_err(|error| OutboundReportError::new(error.to_string()))?
                .iter()
                .any(|job| job.id == job_id);
            let is_terminal = self
                .storage
                .has_completed_idempotency_key(idempotency_key)
                .await
                .map_err(|error| OutboundReportError::new(error.to_string()))?;
            if !is_pending && !is_terminal {
                return Err(OutboundReportError::new(
                    "receipt report preceded its durable commit",
                ));
            }
        }

        self.messages.lock().await.push(message.clone());
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum RejectedReport {
    Receipt,
    Terminal,
}

struct RejectingReporter {
    recorder: Arc<RecordingReporter>,
    rejected: RejectedReport,
}

#[async_trait]
impl OutboundReporter for RejectingReporter {
    async fn report(&self, message: &AgentMessage) -> Result<(), OutboundReportError> {
        let rejected = match self.rejected {
            RejectedReport::Receipt => matches!(&message.kind, AgentMessageKind::JobReceived(_)),
            RejectedReport::Terminal => matches!(
                &message.kind,
                AgentMessageKind::JobSubmitted(_) | AgentMessageKind::JobFailed(_)
            ),
        };
        if rejected {
            return Err(OutboundReportError::new("simulated transport disconnect"));
        }
        self.recorder.report(message).await
    }
}

struct Harness {
    agent: Arc<Agent>,
    storage: Arc<SqliteStorage>,
    spooler: Arc<VirtualSpooler>,
    reporter: Arc<RecordingReporter>,
}

async fn harness(simulation: VirtualSimulation) -> Harness {
    let storage = Arc::new(SqliteStorage::in_memory().await.unwrap());
    build_harness(storage, simulation).await
}

async fn build_harness(storage: Arc<SqliteStorage>, simulation: VirtualSimulation) -> Harness {
    let reporter = Arc::new(RecordingReporter::new(storage.clone()));
    let (agent, spooler) = build_agent(
        storage.clone(),
        simulation,
        Arc::clone(&reporter) as Arc<dyn OutboundReporter>,
    )
    .await;

    Harness {
        agent,
        storage,
        spooler,
        reporter,
    }
}

async fn build_agent(
    storage: Arc<SqliteStorage>,
    simulation: VirtualSimulation,
    reporter: Arc<dyn OutboundReporter>,
) -> (Arc<Agent>, Arc<VirtualSpooler>) {
    let spooler = Arc::new(VirtualSpooler::default());
    spooler.set_simulation(simulation).await;
    let mut registry = SpoolerRegistry::new();
    registry.register(spooler.clone());
    let resolver = Arc::new(
        StaticPrinterResolver::new([virtual_printer()])
            .expect("the test printer reference must be valid"),
    );
    let agent = AgentBuilder::new()
        .repository(storage.clone())
        .printer_resolver(resolver)
        .spooler_registry(Arc::new(registry))
        .outbound_reporter(reporter)
        .build()
        .unwrap();

    (Arc::new(agent), spooler)
}

fn virtual_printer() -> PrinterRef {
    PrinterRef {
        id: PrinterId::new(PRINTER_ID).unwrap(),
        display_name: "Virtual receipt printer".to_owned(),
        connection: PrinterConnection::Virtual {
            printer_id: "virtual_backend_1".to_owned(),
        },
        enabled: true,
    }
}

fn print_job_message(message_id: &str, job_id: &str, idempotency_key: &str) -> ServerMessage {
    ServerMessage {
        protocol_version: ProtocolVersion::CURRENT,
        message_id: message_id.to_owned(),
        sent_at: "2026-07-28T10:00:00Z".to_owned(),
        correlation_id: None,
        kind: ServerMessageKind::PrintJob(PrintJob {
            job_id: job_id.to_owned(),
            idempotency_key: idempotency_key.to_owned(),
            printer_id: PRINTER_ID.to_owned(),
            created_at: "2026-07-28T09:59:00Z".to_owned(),
            document: PrintDocument {
                width: ReceiptWidth::Mm80,
                sections: vec![PrintSection::Text {
                    value: "Durable receipt".to_owned(),
                    align: None,
                    bold: Some(true),
                }],
            },
            metadata: None,
        }),
    }
}

#[tokio::test]
async fn persists_before_receipt_and_terminal_reports() {
    let harness = harness(VirtualSimulation::AlwaysSucceed).await;
    let mut events = harness.agent.handle().subscribe_job_events();

    let result = harness
        .agent
        .handle_print_job(&print_job_message(
            "server_message_1",
            JOB_ID,
            IDEMPOTENCY_KEY,
        ))
        .await
        .unwrap();

    assert!(matches!(result.receipt, ReceiveJobOutcome::Inserted { .. }));
    assert!(matches!(
        result.outcomes.as_slice(),
        [ProcessOutcome::Submitted { .. }]
    ));
    assert_eq!(
        harness.reporter.message_types().await,
        ["agent.job_received", "agent.job_submitted"]
    );
    assert!(
        harness
            .storage
            .pending_outbound_statuses(DEFAULT_OUTBOUND_STATUS_BATCH_SIZE)
            .await
            .unwrap()
            .is_empty(),
        "one successful report must acknowledge exactly one terminal outbox row"
    );
    assert!(
        harness
            .storage
            .has_completed_idempotency_key(IDEMPOTENCY_KEY)
            .await
            .unwrap()
    );
    assert_eq!(harness.spooler.history().await.len(), 1);
    assert_eq!(harness.agent.handle().snapshot().await.pending_jobs, 0);

    let mut observed = Vec::new();
    while let Ok(event) = events.try_recv() {
        observed.push(event);
    }
    assert_eq!(
        observed,
        [
            AgentEvent::JobReceived {
                job_id: PrintJobId::new(JOB_ID).unwrap(),
            },
            AgentEvent::PendingJobsChanged { pending_jobs: 1 },
            AgentEvent::JobSubmitting {
                job_id: PrintJobId::new(JOB_ID).unwrap(),
            },
            AgentEvent::JobSubmitted {
                job_id: PrintJobId::new(JOB_ID).unwrap(),
            },
            AgentEvent::PendingJobsChanged { pending_jobs: 0 },
        ]
    );
}

#[tokio::test]
async fn local_test_submission_never_creates_server_job_or_outbox_state() {
    let harness = harness(VirtualSimulation::AlwaysSucceed).await;
    let ServerMessageKind::PrintJob(job) =
        print_job_message("local_message_1", "job_local_1", "local_test_1").kind
    else {
        unreachable!("fixture is a print job");
    };

    let receipt = harness.agent.submit_local_test(&job).await.unwrap();

    assert_eq!(receipt.backend, "virtual");
    assert_eq!(harness.spooler.history().await.len(), 1);
    assert!(harness.reporter.messages().await.is_empty());
    assert!(harness.storage.pending().await.unwrap().is_empty());
    assert!(
        !harness
            .storage
            .has_completed_idempotency_key(&job.idempotency_key)
            .await
            .unwrap()
    );
    assert!(
        harness
            .storage
            .pending_outbound_statuses(DEFAULT_OUTBOUND_STATUS_BATCH_SIZE)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn exact_and_idempotency_duplicates_never_resubmit() {
    let harness = harness(VirtualSimulation::AlwaysSucceed).await;
    let original = print_job_message("server_message_1", JOB_ID, IDEMPOTENCY_KEY);
    harness.agent.handle_print_job(&original).await.unwrap();

    let duplicate = harness
        .agent
        .handle_print_job(&print_job_message(
            "server_message_2",
            JOB_ID,
            IDEMPOTENCY_KEY,
        ))
        .await
        .unwrap();
    assert!(matches!(
        duplicate.receipt,
        ReceiveJobOutcome::DuplicateJob {
            state: JobState::Submitted,
            ..
        }
    ));
    assert!(duplicate.outcomes.is_empty());

    let conflict = harness
        .agent
        .handle_print_job(&print_job_message(
            "server_message_3",
            "job_2",
            IDEMPOTENCY_KEY,
        ))
        .await
        .unwrap();
    assert!(matches!(
        conflict.receipt,
        ReceiveJobOutcome::DuplicateIdempotency {
            state: JobState::Submitted,
            ..
        }
    ));
    assert!(conflict.outcomes.is_empty());

    assert_eq!(harness.spooler.history().await.len(), 1);
    assert_eq!(
        harness.reporter.message_types().await,
        [
            "agent.job_received",
            "agent.job_submitted",
            "agent.job_received",
        ]
    );
}

#[tokio::test]
async fn persists_failure_before_reporting_and_deduplicates_redelivery() {
    let harness = harness(VirtualSimulation::AlwaysFail).await;
    let message = print_job_message("server_message_1", JOB_ID, IDEMPOTENCY_KEY);

    let result = harness.agent.handle_print_job(&message).await.unwrap();
    assert!(matches!(
        result.outcomes.as_slice(),
        [ProcessOutcome::Failed {
            message: Some(_),
            ..
        }]
    ));
    assert!(
        harness
            .storage
            .has_completed_idempotency_key(IDEMPOTENCY_KEY)
            .await
            .unwrap()
    );
    assert_eq!(
        harness.reporter.message_types().await,
        ["agent.job_received", "agent.job_failed"]
    );

    let reports = harness.reporter.messages().await;
    let AgentMessageKind::JobFailed(failure) = &reports[1].kind else {
        panic!("expected a durable failure report");
    };
    assert_eq!(failure.error.code, "spooler.failed");
    assert!(failure.error.retryable);

    let duplicate = harness
        .agent
        .handle_print_job(&print_job_message(
            "server_message_2",
            JOB_ID,
            IDEMPOTENCY_KEY,
        ))
        .await
        .unwrap();
    assert!(matches!(
        duplicate.receipt,
        ReceiveJobOutcome::DuplicateJob {
            state: JobState::Failed,
            ..
        }
    ));
    assert_eq!(harness.spooler.history().await.len(), 1);
}

#[tokio::test]
async fn cooperatively_cancels_an_active_virtual_submission() {
    let harness = harness(VirtualSimulation::Delay(Duration::from_secs(5))).await;
    let mut events = harness.agent.handle().subscribe_job_events();
    let agent = Arc::clone(&harness.agent);
    let task = tokio::spawn(async move {
        agent
            .handle_print_job(&print_job_message(
                "server_message_1",
                JOB_ID,
                IDEMPOTENCY_KEY,
            ))
            .await
    });

    timeout(Duration::from_secs(2), async {
        loop {
            if matches!(
                events.recv().await.unwrap(),
                AgentEvent::JobSubmitting { .. }
            ) {
                break;
            }
        }
    })
    .await
    .expect("submission did not become active");

    assert!(matches!(
        harness.agent.cancel_job(JOB_ID).await.unwrap(),
        CancelOutcome::CancellationRequested { .. }
    ));
    let result = timeout(Duration::from_secs(2), task)
        .await
        .expect("cancelled submission did not stop")
        .unwrap()
        .unwrap();
    assert!(matches!(
        result.outcomes.as_slice(),
        [ProcessOutcome::Cancelled { .. }]
    ));
    assert!(
        harness
            .storage
            .has_completed_idempotency_key(IDEMPOTENCY_KEY)
            .await
            .unwrap()
    );
    assert!(harness.spooler.history().await.is_empty());
    assert_eq!(
        harness.reporter.message_types().await,
        ["agent.job_received"]
    );
    assert_eq!(harness.agent.handle().snapshot().await.pending_jobs, 0);
}

#[tokio::test]
async fn reconnect_resends_a_lost_receipt_before_processing_pending_work() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("oppa-agent.sqlite3");

    {
        let storage = Arc::new(
            SqliteStorage::open(&database_path, DEFAULT_MAX_PENDING_JOBS)
                .await
                .unwrap(),
        );
        let recorder = Arc::new(RecordingReporter::new(storage.clone()));
        let reporter: Arc<dyn OutboundReporter> = Arc::new(RejectingReporter {
            recorder,
            rejected: RejectedReport::Receipt,
        });
        let (agent, spooler) =
            build_agent(storage.clone(), VirtualSimulation::AlwaysSucceed, reporter).await;

        let error = agent
            .handle_print_job(&print_job_message(
                "server_message_1",
                JOB_ID,
                IDEMPOTENCY_KEY,
            ))
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            AgentRuntimeError::Reporting {
                message: AgentMessage {
                    kind: AgentMessageKind::JobReceived(_),
                    ..
                },
                ..
            }
        ));
        assert!(spooler.history().await.is_empty());
        assert_eq!(storage.pending().await.unwrap().len(), 1);
    }

    let storage = Arc::new(
        SqliteStorage::open(&database_path, DEFAULT_MAX_PENDING_JOBS)
            .await
            .unwrap(),
    );
    let harness = build_harness(storage, VirtualSimulation::AlwaysSucceed).await;

    assert_eq!(harness.agent.replay_outbound_reports().await.unwrap(), 0);
    let summary = harness.agent.recover().await.unwrap();

    assert_eq!(summary.recovery.pending_jobs, 1);
    assert!(matches!(
        summary.outcomes.as_slice(),
        [ProcessOutcome::Submitted { .. }]
    ));
    assert_eq!(
        harness.reporter.message_types().await,
        ["agent.job_received", "agent.job_submitted"]
    );
    assert_eq!(harness.spooler.history().await.len(), 1);
}

#[tokio::test]
async fn restart_replays_each_terminal_outbox_status_once_then_acknowledges_it() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("oppa-agent.sqlite3");

    {
        let storage = Arc::new(
            SqliteStorage::open(&database_path, DEFAULT_MAX_PENDING_JOBS)
                .await
                .unwrap(),
        );
        let recorder = Arc::new(RecordingReporter::new(storage.clone()));
        let reporter: Arc<dyn OutboundReporter> = Arc::new(RejectingReporter {
            recorder: recorder.clone(),
            rejected: RejectedReport::Terminal,
        });
        let (agent, spooler) =
            build_agent(storage.clone(), VirtualSimulation::AlwaysSucceed, reporter).await;

        let error = agent
            .handle_print_job(&print_job_message(
                "server_message_1",
                JOB_ID,
                IDEMPOTENCY_KEY,
            ))
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            AgentRuntimeError::Reporting {
                message: AgentMessage {
                    kind: AgentMessageKind::JobSubmitted(_),
                    ..
                },
                ..
            }
        ));
        assert_eq!(recorder.message_types().await, ["agent.job_received"]);
        assert_eq!(spooler.history().await.len(), 1);
        assert_eq!(
            storage
                .pending_outbound_statuses(DEFAULT_OUTBOUND_STATUS_BATCH_SIZE)
                .await
                .unwrap()
                .len(),
            1
        );
    }

    let storage = Arc::new(
        SqliteStorage::open(&database_path, DEFAULT_MAX_PENDING_JOBS)
            .await
            .unwrap(),
    );
    let harness = build_harness(storage, VirtualSimulation::AlwaysSucceed).await;

    assert_eq!(harness.agent.replay_outbound_reports().await.unwrap(), 1);
    assert_eq!(harness.agent.replay_outbound_reports().await.unwrap(), 0);
    assert_eq!(
        harness.reporter.message_types().await,
        ["agent.job_submitted"]
    );
    assert!(
        harness
            .storage
            .pending_outbound_statuses(DEFAULT_OUTBOUND_STATUS_BATCH_SIZE)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(harness.agent.recover().await.unwrap().outcomes.is_empty());
    assert!(harness.spooler.history().await.is_empty());
}

#[tokio::test]
async fn recovers_and_replays_an_interrupted_submission_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("oppa-agent.sqlite3");
    let message = print_job_message("server_message_1", JOB_ID, IDEMPOTENCY_KEY);

    {
        let storage = Arc::new(
            SqliteStorage::open(&database_path, DEFAULT_MAX_PENDING_JOBS)
                .await
                .unwrap(),
        );
        let harness = build_harness(storage.clone(), VirtualSimulation::AlwaysSucceed).await;
        let receipt = harness
            .agent
            .job_processor()
            .receive_print_job(&message)
            .await
            .unwrap();
        assert!(matches!(receipt, ReceiveJobOutcome::Inserted { .. }));
        storage
            .mark_submitting(&PrintJobId::new(JOB_ID).unwrap())
            .await
            .unwrap();
    }

    let storage = Arc::new(
        SqliteStorage::open(&database_path, DEFAULT_MAX_PENDING_JOBS)
            .await
            .unwrap(),
    );
    let harness = build_harness(storage, VirtualSimulation::AlwaysSucceed).await;
    let mut events = harness.agent.handle().subscribe_job_events();
    let summary = harness.agent.recover().await.unwrap();

    assert_eq!(summary.recovery.recovered_submissions, 1);
    assert_eq!(summary.recovery.pending_jobs, 1);
    assert!(matches!(
        summary.outcomes.as_slice(),
        [ProcessOutcome::Submitted { .. }]
    ));
    assert_eq!(harness.spooler.history().await.len(), 1);
    assert_eq!(
        harness.reporter.message_types().await,
        ["agent.job_received", "agent.job_submitted"]
    );
    assert_eq!(harness.agent.handle().snapshot().await.pending_jobs, 0);

    let mut observed = Vec::new();
    while let Ok(event) = events.try_recv() {
        observed.push(event);
    }
    assert!(observed.contains(&AgentEvent::RecoveryCompleted {
        recovered_submissions: 1,
        pending_jobs: 1,
    }));
    assert!(observed.contains(&AgentEvent::JobSubmitted {
        job_id: PrintJobId::new(JOB_ID).unwrap(),
    }));
}

#[tokio::test]
async fn reports_uncertainty_when_an_interrupted_submission_replay_fails() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("oppa-agent.sqlite3");
    let message = print_job_message("server_message_1", JOB_ID, IDEMPOTENCY_KEY);

    {
        let storage = Arc::new(
            SqliteStorage::open(&database_path, DEFAULT_MAX_PENDING_JOBS)
                .await
                .unwrap(),
        );
        let harness = build_harness(storage.clone(), VirtualSimulation::AlwaysSucceed).await;
        harness
            .agent
            .job_processor()
            .receive_print_job(&message)
            .await
            .unwrap();
        storage
            .mark_submitting(&PrintJobId::new(JOB_ID).unwrap())
            .await
            .unwrap();
    }

    let storage = Arc::new(
        SqliteStorage::open(&database_path, DEFAULT_MAX_PENDING_JOBS)
            .await
            .unwrap(),
    );
    let harness = build_harness(storage, VirtualSimulation::AlwaysFail).await;
    let summary = harness.agent.recover().await.unwrap();
    assert!(matches!(
        summary.outcomes.as_slice(),
        [ProcessOutcome::Failed {
            message: Some(_),
            ..
        }]
    ));

    let reports = harness.reporter.messages().await;
    assert!(matches!(&reports[0].kind, AgentMessageKind::JobReceived(_)));
    let AgentMessageKind::JobFailed(failure) = &reports[1].kind else {
        panic!("expected an uncertain recovery failure");
    };
    assert_eq!(failure.error.code, "job.recovery_uncertain");
    assert!(!failure.error.retryable);
    assert!(
        failure
            .error
            .message
            .contains("may already have reached its backend")
    );
}
