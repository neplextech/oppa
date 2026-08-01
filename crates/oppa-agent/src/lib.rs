//! Shell-independent OPPA agent lifecycle and orchestration.
//!
//! The desktop application hosts this crate through narrow commands, but the
//! state machine itself has no Tauri dependency.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::sync::Arc;

use oppa_core::{JobState, PrintJobId};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{RwLock, broadcast, watch};

mod processor;
mod runtime;

pub use processor::{
    CancelOutcome, JobFlowResult, JobProcessingError, JobProcessor, LocalTestPrintError,
    PrinterResolutionError, PrinterResolver, ProcessOutcome, ReceiveJobOutcome, RecoverySummary,
    StaticPrinterResolver,
};
pub use runtime::{
    Agent, AgentBuildError, AgentBuilder, AgentRuntimeError, OutboundReportError, OutboundReporter,
    ServerJobOutcome,
};

const EVENT_CHANNEL_CAPACITY: usize = 256;
const MAX_ACTIVE_ERRORS: usize = 32;
const MAX_ACTIVE_ERROR_BYTES: usize = 1_000;

/// Stable lifecycle states exposed to hosts and diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    /// No usable agent credential has been configured.
    Unconfigured,
    /// Discovery and one-time pairing are in progress.
    Pairing,
    /// The agent is configured but has no gateway connection.
    Disconnected,
    /// Transport negotiation or authentication is in progress.
    Connecting,
    /// The gateway and local processing loop are healthy.
    Connected,
    /// Core processing continues while one subsystem needs attention.
    Degraded,
    /// New work is stopped while resources close.
    ShuttingDown,
}

/// A host-facing projection of agent state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSnapshot {
    /// Current lifecycle state.
    pub state: AgentState,
    /// Number of durably stored jobs that have not reached a terminal state.
    pub pending_jobs: usize,
    /// Sanitized active errors suitable for a local UI.
    pub active_errors: Vec<String>,
}

impl Default for AgentSnapshot {
    fn default() -> Self {
        Self {
            state: AgentState::Unconfigured,
            pending_jobs: 0,
            active_errors: Vec::new(),
        }
    }
}

/// Bounded, payload-free job lifecycle event exposed to hosts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    /// The number of durable non-terminal jobs changed.
    PendingJobsChanged {
        /// Latest known non-terminal job count.
        pending_jobs: usize,
    },
    /// A new job was durably persisted and may be acknowledged.
    JobReceived {
        /// Durable job identity.
        job_id: PrintJobId,
    },
    /// An exact at-least-once redelivery was observed without resubmission.
    DuplicateJob {
        /// Redelivered durable job identity.
        job_id: PrintJobId,
        /// Current durable state.
        state: JobState,
    },
    /// An idempotency key was reused by a different incoming job.
    DuplicateIdempotency {
        /// Incoming job that was not persisted.
        job_id: PrintJobId,
        /// Existing durable job that owns the key.
        existing_job_id: PrintJobId,
        /// Existing durable state.
        state: JobState,
    },
    /// A received job was atomically claimed for submission.
    JobSubmitting {
        /// Durable job identity.
        job_id: PrintJobId,
    },
    /// A backend accepted a job and the result was persisted.
    JobSubmitted {
        /// Durable job identity.
        job_id: PrintJobId,
    },
    /// A job failure was persisted.
    JobFailed {
        /// Durable job identity.
        job_id: PrintJobId,
        /// Stable sanitized failure category.
        code: String,
        /// Whether a future explicit retry may be useful.
        retryable: bool,
    },
    /// Cooperative cancellation was requested for an active submission.
    CancellationRequested {
        /// Durable job identity.
        job_id: PrintJobId,
    },
    /// Cancellation reached durable state before backend acceptance.
    JobCancelled {
        /// Durable job identity.
        job_id: PrintJobId,
    },
    /// Startup recovery completed before pending work was replayed.
    RecoveryCompleted {
        /// Interrupted submissions moved back to received.
        recovered_submissions: usize,
        /// Jobs available for replay.
        pending_jobs: usize,
    },
}

/// Lifecycle transition failures.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AgentError {
    /// The requested state cannot follow the current state.
    #[error("invalid agent transition from {from:?} to {to:?}")]
    InvalidTransition {
        /// Current state.
        from: AgentState,
        /// Requested state.
        to: AgentState,
    },
}

/// Cloneable host handle for state inspection and shutdown.
#[derive(Clone)]
pub struct AgentHandle {
    state: Arc<RwLock<AgentSnapshot>>,
    events: watch::Sender<AgentSnapshot>,
    job_events: broadcast::Sender<AgentEvent>,
}

impl AgentHandle {
    /// Creates a lifecycle handle with the given initial snapshot.
    #[must_use]
    pub fn new(mut initial: AgentSnapshot) -> Self {
        initial.active_errors = sanitize_active_errors(initial.active_errors);
        let (events, _) = watch::channel(initial.clone());
        let (job_events, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Self {
            state: Arc::new(RwLock::new(initial)),
            events,
            job_events,
        }
    }

    /// Returns the latest host-facing state.
    pub async fn snapshot(&self) -> AgentSnapshot {
        self.state.read().await.clone()
    }

    /// Subscribes to state changes without exposing internal services.
    #[must_use]
    pub fn subscribe(&self) -> watch::Receiver<AgentSnapshot> {
        self.events.subscribe()
    }

    /// Subscribes to bounded, payload-free job lifecycle events.
    ///
    /// Slow consumers receive a broadcast lag error rather than causing an
    /// unbounded in-memory queue.
    #[must_use]
    pub fn subscribe_job_events(&self) -> broadcast::Receiver<AgentEvent> {
        self.job_events.subscribe()
    }

    /// Applies a validated lifecycle transition and notifies subscribers.
    ///
    /// # Errors
    ///
    /// Returns [`AgentError::InvalidTransition`] when `next` cannot follow the
    /// current lifecycle state.
    pub async fn transition(&self, next: AgentState) -> Result<(), AgentError> {
        let mut snapshot = self.state.write().await;
        if !can_transition(snapshot.state, next) {
            return Err(AgentError::InvalidTransition {
                from: snapshot.state,
                to: next,
            });
        }
        snapshot.state = next;
        self.events.send_replace(snapshot.clone());
        Ok(())
    }

    /// Updates the pending count after a durable repository operation.
    pub async fn set_pending_jobs(&self, pending_jobs: usize) {
        let mut snapshot = self.state.write().await;
        if snapshot.pending_jobs == pending_jobs {
            return;
        }
        snapshot.pending_jobs = pending_jobs;
        self.events.send_replace(snapshot.clone());
        let _ = self
            .job_events
            .send(AgentEvent::PendingJobsChanged { pending_jobs });
    }

    /// Replaces sanitized active errors and publishes a new snapshot.
    pub async fn set_active_errors(&self, active_errors: Vec<String>) {
        let mut snapshot = self.state.write().await;
        snapshot.active_errors = sanitize_active_errors(active_errors);
        self.events.send_replace(snapshot.clone());
    }

    pub(crate) fn publish_job_event(&self, event: AgentEvent) {
        let _ = self.job_events.send(event);
    }
}

fn sanitize_active_errors(errors: Vec<String>) -> Vec<String> {
    errors
        .into_iter()
        .filter_map(|error| {
            let sanitized: String = error
                .chars()
                .map(|character| {
                    if character.is_control() {
                        ' '
                    } else {
                        character
                    }
                })
                .collect();
            let sanitized = sanitized.trim();
            if sanitized.is_empty() {
                return None;
            }
            Some(truncate_utf8(sanitized, MAX_ACTIVE_ERROR_BYTES).to_owned())
        })
        .take(MAX_ACTIVE_ERRORS)
        .collect()
}

fn truncate_utf8(value: &str, maximum: usize) -> &str {
    if value.len() <= maximum {
        return value;
    }
    let mut boundary = maximum;
    while !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    &value[..boundary]
}

fn can_transition(from: AgentState, to: AgentState) -> bool {
    if from == to {
        return true;
    }
    if to == AgentState::ShuttingDown {
        return true;
    }
    matches!(
        (from, to),
        (AgentState::Unconfigured, AgentState::Pairing)
            | (
                AgentState::Pairing,
                AgentState::Unconfigured | AgentState::Disconnected
            )
            | (AgentState::Disconnected, AgentState::Connecting)
            | (
                AgentState::Connecting | AgentState::Degraded,
                AgentState::Connected
            )
            | (
                AgentState::Connecting | AgentState::Connected | AgentState::Degraded,
                AgentState::Disconnected
            )
            | (
                AgentState::Disconnected
                    | AgentState::Connecting
                    | AgentState::Connected
                    | AgentState::Degraded,
                AgentState::Unconfigured
            )
            | (AgentState::Connected, AgentState::Degraded)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn publishes_valid_state_transitions() {
        let handle = AgentHandle::new(AgentSnapshot {
            state: AgentState::Disconnected,
            ..AgentSnapshot::default()
        });
        let mut events = handle.subscribe();

        handle.transition(AgentState::Connecting).await.unwrap();
        events.changed().await.unwrap();

        assert_eq!(events.borrow().state, AgentState::Connecting);
    }

    #[tokio::test]
    async fn rejects_impossible_state_transitions() {
        let handle = AgentHandle::new(AgentSnapshot::default());
        let error = handle.transition(AgentState::Connected).await.unwrap_err();

        assert_eq!(
            error,
            AgentError::InvalidTransition {
                from: AgentState::Unconfigured,
                to: AgentState::Connected,
            }
        );
    }

    #[tokio::test]
    async fn revoked_credentials_reset_connected_lifecycle_to_unconfigured() {
        for state in [
            AgentState::Disconnected,
            AgentState::Connecting,
            AgentState::Connected,
            AgentState::Degraded,
        ] {
            let handle = AgentHandle::new(AgentSnapshot {
                state,
                ..AgentSnapshot::default()
            });
            handle
                .transition(AgentState::Unconfigured)
                .await
                .expect("credential revocation must reset lifecycle");
            assert_eq!(handle.snapshot().await.state, AgentState::Unconfigured);
        }
    }
}
