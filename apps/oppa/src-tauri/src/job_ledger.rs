use std::collections::VecDeque;

use oppa_agent::AgentEvent;
use oppa_storage::SqliteStorage;
use serde_json::Value;
use tokio::sync::RwLock;

use crate::{
    error::{CommandError, sanitize},
    models::{DesktopJobState, JobSummary},
};

const JOB_HISTORY_SETTING: &str = "desktop.recent-jobs.v1";
const MAX_RECENT_JOBS: usize = 100;

/// Bounded, payload-free job summaries for the desktop UI.
pub struct JobLedger {
    storage: SqliteStorage,
    jobs: RwLock<VecDeque<JobSummary>>,
}

impl JobLedger {
    pub async fn load(storage: SqliteStorage) -> Result<Self, CommandError> {
        let jobs = match storage
            .setting(JOB_HISTORY_SETTING)
            .await
            .map_err(|error| CommandError::internal(error.to_string()))?
        {
            Some(value) => {
                serde_json::from_value::<VecDeque<JobSummary>>(value).map_err(|error| {
                    CommandError::internal(format!("job history is invalid: {error}"))
                })?
            }
            None => VecDeque::new(),
        };
        Ok(Self {
            storage,
            jobs: RwLock::new(jobs.into_iter().take(MAX_RECENT_JOBS).collect()),
        })
    }

    pub async fn list(&self) -> Vec<JobSummary> {
        self.jobs.read().await.iter().cloned().collect()
    }

    pub async fn insert(&self, job: JobSummary) -> Result<(), CommandError> {
        let mut jobs = self.jobs.write().await;
        if let Some(index) = jobs.iter().position(|existing| existing.id == job.id) {
            let existing = &jobs[index];
            if existing.printer_id != job.printer_id
                || existing.printer_name != job.printer_name
                || existing.idempotency_key != job.idempotency_key
            {
                return Err(CommandError::new(
                    "job_identity_conflict",
                    "A job with this ID already exists with different immutable metadata.",
                ));
            }
            if let Some(mut existing) = jobs.remove(index) {
                merge_delivery(&mut existing, job);
                jobs.push_front(existing);
            }
        } else {
            jobs.push_front(job);
        }
        while jobs.len() > MAX_RECENT_JOBS {
            jobs.pop_back();
        }
        drop(jobs);
        self.persist().await
    }

    pub async fn fail_if_non_terminal(
        &self,
        job_id: &str,
        error: impl AsRef<str>,
    ) -> Result<(), CommandError> {
        let mut jobs = self.jobs.write().await;
        let Some(job) = jobs.iter_mut().find(|job| job.id == job_id) else {
            return Ok(());
        };
        if is_terminal(job.state) {
            return Ok(());
        }
        job.state = DesktopJobState::Failed;
        job.updated_at = oppa_core::Timestamp::now().to_string();
        job.error = Some(sanitize(error.as_ref()));
        drop(jobs);
        self.persist().await
    }

    pub async fn clear(&self) -> Result<(), CommandError> {
        self.jobs.write().await.clear();
        self.persist().await
    }

    pub async fn restore_pending(&self, pending: Vec<JobSummary>) -> Result<(), CommandError> {
        let mut jobs = self.jobs.write().await;
        for job in pending {
            if let Some(existing) = jobs.iter_mut().find(|existing| existing.id == job.id) {
                merge_delivery(existing, job);
            } else {
                jobs.push_front(job);
            }
        }
        let mut ordered = jobs.drain(..).collect::<Vec<_>>();
        ordered.sort_by(|left, right| right.received_at.cmp(&left.received_at));
        *jobs = ordered.into_iter().take(MAX_RECENT_JOBS).collect();
        drop(jobs);
        self.persist().await
    }

    pub async fn update(
        &self,
        job_id: &str,
        state: DesktopJobState,
        error: Option<String>,
        attempts: Option<u32>,
        updated_at: String,
    ) -> Result<(), CommandError> {
        let mut jobs = self.jobs.write().await;
        let Some(job) = jobs.iter_mut().find(|job| job.id == job_id) else {
            return Ok(());
        };
        if is_terminal(job.state) {
            return Ok(());
        }
        job.state = state;
        job.updated_at = updated_at;
        job.error = error.map(|error| sanitize(&error));
        if let Some(attempts) = attempts {
            job.attempts = attempts;
        }
        drop(jobs);
        self.persist().await
    }

    pub async fn apply_event(&self, event: &AgentEvent) -> Result<(), CommandError> {
        let now = oppa_core::Timestamp::now().to_string();
        match event {
            AgentEvent::JobReceived { job_id } => {
                self.update(job_id.as_str(), DesktopJobState::Received, None, None, now)
                    .await
            }
            AgentEvent::JobSubmitting { job_id } => {
                self.update(
                    job_id.as_str(),
                    DesktopJobState::Received,
                    None,
                    Some(1),
                    now,
                )
                .await
            }
            AgentEvent::JobSubmitted { job_id } => {
                self.update(job_id.as_str(), DesktopJobState::Submitted, None, None, now)
                    .await
            }
            AgentEvent::JobFailed {
                job_id,
                code,
                retryable: _,
            } => {
                self.update(
                    job_id.as_str(),
                    DesktopJobState::Failed,
                    Some(format!("Print job failed ({code}).")),
                    None,
                    now,
                )
                .await
            }
            AgentEvent::JobCancelled { job_id } => {
                self.update(job_id.as_str(), DesktopJobState::Cancelled, None, None, now)
                    .await
            }
            AgentEvent::PendingJobsChanged { .. }
            | AgentEvent::DuplicateJob { .. }
            | AgentEvent::DuplicateIdempotency { .. }
            | AgentEvent::CancellationRequested { .. }
            | AgentEvent::RecoveryCompleted { .. } => Ok(()),
        }
    }

    async fn persist(&self) -> Result<(), CommandError> {
        let value: Value = serde_json::to_value(self.jobs.read().await.clone())
            .map_err(|error| CommandError::internal(error.to_string()))?;
        self.storage
            .set_setting(JOB_HISTORY_SETTING, &value)
            .await
            .map_err(|error| CommandError::internal(error.to_string()))
    }
}

fn merge_delivery(existing: &mut JobSummary, incoming: JobSummary) {
    if !is_terminal(existing.state) {
        existing.state = incoming.state;
        existing.updated_at = incoming.updated_at;
        existing.attempts = existing.attempts.max(incoming.attempts);
        existing.error = incoming.error;
    }
}

fn is_terminal(state: DesktopJobState) -> bool {
    matches!(
        state,
        DesktopJobState::Submitted | DesktopJobState::Failed | DesktopJobState::Cancelled
    )
}

#[cfg(test)]
mod tests {
    use oppa_storage::SqliteStorage;

    use super::*;

    #[tokio::test]
    async fn keeps_newest_jobs_first_and_updates_without_payloads() {
        let storage = SqliteStorage::in_memory().await.expect("storage");
        let ledger = JobLedger::load(storage).await.expect("ledger");
        for index in 0..3 {
            ledger
                .insert(JobSummary {
                    id: format!("job-{index}"),
                    printer_id: "printer".to_owned(),
                    printer_name: "Receipt".to_owned(),
                    idempotency_key: format!("key-{index}"),
                    state: DesktopJobState::Received,
                    received_at: "2026-01-01T00:00:00Z".to_owned(),
                    updated_at: "2026-01-01T00:00:00Z".to_owned(),
                    attempts: 0,
                    error: None,
                })
                .await
                .expect("insert");
        }

        let jobs = ledger.list().await;
        assert_eq!(jobs[0].id, "job-2");
        ledger
            .update(
                "job-2",
                DesktopJobState::Failed,
                Some("failure".to_owned()),
                Some(1),
                "2026-01-01T00:00:01Z".to_owned(),
            )
            .await
            .expect("update");
        assert_eq!(ledger.list().await[0].state, DesktopJobState::Failed);
    }

    #[tokio::test]
    async fn duplicate_delivery_does_not_regress_terminal_history() {
        let storage = SqliteStorage::in_memory().await.expect("storage");
        let ledger = JobLedger::load(storage).await.expect("ledger");
        let mut summary = JobSummary {
            id: "job-terminal".to_owned(),
            printer_id: "printer".to_owned(),
            printer_name: "Receipt".to_owned(),
            idempotency_key: "key".to_owned(),
            state: DesktopJobState::Submitted,
            received_at: "2026-01-01T00:00:00Z".to_owned(),
            updated_at: "2026-01-01T00:00:01Z".to_owned(),
            attempts: 1,
            error: None,
        };
        ledger.insert(summary.clone()).await.expect("terminal");
        summary.state = DesktopJobState::Queued;
        summary.updated_at = "2026-01-01T00:00:02Z".to_owned();
        summary.attempts = 0;
        ledger.insert(summary).await.expect("duplicate");

        let retained = &ledger.list().await[0];
        assert_eq!(retained.state, DesktopJobState::Submitted);
        assert_eq!(retained.attempts, 1);
        assert_eq!(retained.updated_at, "2026-01-01T00:00:01Z");
    }

    #[tokio::test]
    async fn conflicting_redelivery_cannot_rewrite_immutable_history() {
        let storage = SqliteStorage::in_memory().await.expect("storage");
        let ledger = JobLedger::load(storage).await.expect("ledger");
        let original = JobSummary {
            id: "job-terminal".to_owned(),
            printer_id: "printer-original".to_owned(),
            printer_name: "Original".to_owned(),
            idempotency_key: "key-original".to_owned(),
            state: DesktopJobState::Submitted,
            received_at: "2026-01-01T00:00:00Z".to_owned(),
            updated_at: "2026-01-01T00:00:01Z".to_owned(),
            attempts: 1,
            error: None,
        };
        ledger.insert(original.clone()).await.expect("original");
        let mut conflicting = original.clone();
        conflicting.printer_id = "printer-attacker".to_owned();
        conflicting.printer_name = "Attacker".to_owned();
        conflicting.idempotency_key = "key-attacker".to_owned();

        assert!(ledger.insert(conflicting).await.is_err());
        assert_eq!(ledger.list().await, vec![original]);
    }
}
