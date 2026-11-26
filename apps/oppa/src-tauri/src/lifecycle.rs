//! Platform-neutral mobile lifecycle policy.
//!
//! The policy is a pure state machine so its reconnect, suspension, and stop
//! semantics are regression-tested on every host. The mobile host feeds it
//! native events and executes the returned actions. It never touches durable
//! job state: recovery and deduplication stay in the agent, which replays
//! pending work after every successful gateway handshake.

// Used by the mobile host; desktop builds compile it for regression tests.
#![cfg_attr(desktop, allow(dead_code))]

use std::time::{Duration, Instant};

use crate::platform::BackgroundExecution;

/// Minimum spacing between lifecycle-triggered reconnects.
const RECONNECT_DEBOUNCE: Duration = Duration::from_secs(2);
/// After this long in the background the gateway socket is presumed stale
/// on platforms that suspend the process.
const SUSPENSION_STALE_AFTER: Duration = Duration::from_secs(5);

/// Native lifecycle input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleEvent {
    Foregrounded,
    Backgrounded,
    NetworkAvailable,
    NetworkLost,
    StopRequested,
    BackgroundTaskExpired,
    /// The agent finished every job that was pending or in flight.
    JobsDrained,
}

/// Agent facts the policy needs to decide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentFacts {
    pub paired: bool,
    pub connected: bool,
    pub pending_jobs: usize,
}

/// Work the host must perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleAction {
    /// Restart the gateway connection; pending jobs are recovered after the
    /// handshake by the agent's durable replay.
    Reconnect,
    /// Ask iOS for bounded time to finish in-flight jobs.
    BeginBackgroundTask,
    /// Return bounded background time to iOS.
    EndBackgroundTask,
    /// Stop the agent and the process (Android notification action).
    Shutdown,
}

/// Tracks suspension and connectivity between native events.
#[derive(Debug)]
pub struct LifecyclePolicy {
    model: BackgroundExecution,
    backgrounded_at: Option<Instant>,
    network_lost: bool,
    last_reconnect: Option<Instant>,
    background_task: bool,
}

impl LifecyclePolicy {
    pub const fn new(model: BackgroundExecution) -> Self {
        Self {
            model,
            backgrounded_at: None,
            network_lost: false,
            last_reconnect: None,
            background_task: false,
        }
    }

    /// Whether the app is currently in the background.
    #[cfg_attr(not(test), allow(dead_code))]
    pub const fn is_backgrounded(&self) -> bool {
        self.backgrounded_at.is_some()
    }

    /// Decides the actions for one event.
    pub fn on_event(
        &mut self,
        event: LifecycleEvent,
        facts: AgentFacts,
        now: Instant,
    ) -> Vec<LifecycleAction> {
        let mut actions = Vec::new();
        match event {
            LifecycleEvent::StopRequested => {
                if self.background_task {
                    self.background_task = false;
                    actions.push(LifecycleAction::EndBackgroundTask);
                }
                actions.push(LifecycleAction::Shutdown);
            }
            LifecycleEvent::NetworkLost => self.network_lost = true,
            LifecycleEvent::NetworkAvailable => {
                // A socket bound to the previous network will not recover on
                // its own; restart instead of waiting for keepalive failure.
                // While iOS is suspended the change stays pending until resume.
                if self.suspended() {
                    self.network_lost = true;
                } else {
                    self.network_lost = false;
                    if facts.paired {
                        self.push_reconnect(&mut actions, now);
                    }
                }
            }
            LifecycleEvent::Backgrounded => {
                if self.backgrounded_at.is_none() {
                    self.backgrounded_at = Some(now);
                }
                if self.model == BackgroundExecution::ForegroundOnly
                    && facts.paired
                    && facts.pending_jobs > 0
                    && !self.background_task
                {
                    self.background_task = true;
                    actions.push(LifecycleAction::BeginBackgroundTask);
                }
            }
            LifecycleEvent::Foregrounded => {
                let away = self
                    .backgrounded_at
                    .take()
                    .map_or(Duration::ZERO, |since| now.saturating_duration_since(since));
                if self.background_task {
                    self.background_task = false;
                    actions.push(LifecycleAction::EndBackgroundTask);
                }
                let stale = self.model == BackgroundExecution::ForegroundOnly
                    && away >= SUSPENSION_STALE_AFTER;
                if facts.paired && (stale || !facts.connected || self.network_lost) {
                    self.network_lost = false;
                    self.push_reconnect(&mut actions, now);
                }
            }
            LifecycleEvent::BackgroundTaskExpired => self.background_task = false,
            LifecycleEvent::JobsDrained => {
                if self.background_task && facts.pending_jobs == 0 {
                    self.background_task = false;
                    actions.push(LifecycleAction::EndBackgroundTask);
                }
            }
        }
        actions
    }

    /// Whether the process is suspended (iOS background) rather than merely
    /// running without a visible UI (Android foreground service).
    fn suspended(&self) -> bool {
        self.model == BackgroundExecution::ForegroundOnly && self.backgrounded_at.is_some()
    }

    fn push_reconnect(&mut self, actions: &mut Vec<LifecycleAction>, now: Instant) {
        if self
            .last_reconnect
            .is_some_and(|last| now.saturating_duration_since(last) < RECONNECT_DEBOUNCE)
        {
            return;
        }
        self.last_reconnect = Some(now);
        actions.push(LifecycleAction::Reconnect);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NO_ACTIONS: Vec<LifecycleAction> = Vec::new();

    const PAIRED_CONNECTED: AgentFacts = AgentFacts {
        paired: true,
        connected: true,
        pending_jobs: 0,
    };

    fn ios() -> LifecyclePolicy {
        LifecyclePolicy::new(BackgroundExecution::ForegroundOnly)
    }

    fn android() -> LifecyclePolicy {
        LifecyclePolicy::new(BackgroundExecution::ForegroundService)
    }

    #[test]
    fn ios_reconnects_after_suspension() {
        let mut policy = ios();
        let start = Instant::now();
        assert_eq!(
            policy.on_event(LifecycleEvent::Backgrounded, PAIRED_CONNECTED, start),
            NO_ACTIONS
        );
        let actions = policy.on_event(
            LifecycleEvent::Foregrounded,
            PAIRED_CONNECTED,
            start + Duration::from_secs(60),
        );
        assert_eq!(actions, [LifecycleAction::Reconnect]);
        assert!(!policy.is_backgrounded());
    }

    #[test]
    fn brief_ios_interruption_keeps_a_healthy_connection() {
        let mut policy = ios();
        let start = Instant::now();
        policy.on_event(LifecycleEvent::Backgrounded, PAIRED_CONNECTED, start);
        let actions = policy.on_event(
            LifecycleEvent::Foregrounded,
            PAIRED_CONNECTED,
            start + Duration::from_secs(1),
        );
        assert_eq!(actions, NO_ACTIONS);
    }

    #[test]
    fn android_foreground_service_keeps_connection_across_background() {
        let mut policy = android();
        let start = Instant::now();
        policy.on_event(LifecycleEvent::Backgrounded, PAIRED_CONNECTED, start);
        let actions = policy.on_event(
            LifecycleEvent::Foregrounded,
            PAIRED_CONNECTED,
            start + Duration::from_secs(3_600),
        );
        assert_eq!(
            actions, NO_ACTIONS,
            "a live service connection is not restarted"
        );
        let disconnected = AgentFacts {
            connected: false,
            ..PAIRED_CONNECTED
        };
        policy.on_event(LifecycleEvent::Backgrounded, disconnected, start);
        let actions = policy.on_event(
            LifecycleEvent::Foregrounded,
            disconnected,
            start + Duration::from_secs(3_610),
        );
        assert_eq!(actions, [LifecycleAction::Reconnect]);
    }

    #[test]
    fn network_change_reconnects_once_within_debounce() {
        let mut policy = android();
        let start = Instant::now();
        policy.on_event(LifecycleEvent::NetworkLost, PAIRED_CONNECTED, start);
        assert_eq!(
            policy.on_event(LifecycleEvent::NetworkAvailable, PAIRED_CONNECTED, start),
            [LifecycleAction::Reconnect]
        );
        assert_eq!(
            policy.on_event(
                LifecycleEvent::NetworkAvailable,
                PAIRED_CONNECTED,
                start + Duration::from_millis(500)
            ),
            NO_ACTIONS
        );
        assert_eq!(
            policy.on_event(
                LifecycleEvent::NetworkAvailable,
                PAIRED_CONNECTED,
                start + Duration::from_secs(5)
            ),
            [LifecycleAction::Reconnect]
        );
    }

    #[test]
    fn suspended_ios_defers_network_reconnect_until_resume() {
        let mut policy = ios();
        let start = Instant::now();
        policy.on_event(LifecycleEvent::Backgrounded, PAIRED_CONNECTED, start);
        policy.on_event(LifecycleEvent::NetworkLost, PAIRED_CONNECTED, start);
        assert_eq!(
            policy.on_event(LifecycleEvent::NetworkAvailable, PAIRED_CONNECTED, start),
            NO_ACTIONS
        );
        assert_eq!(
            policy.on_event(
                LifecycleEvent::Foregrounded,
                PAIRED_CONNECTED,
                start + Duration::from_secs(1)
            ),
            [LifecycleAction::Reconnect]
        );
    }

    #[test]
    fn unpaired_agent_never_reconnects() {
        let unpaired = AgentFacts {
            paired: false,
            connected: false,
            pending_jobs: 0,
        };
        let mut policy = ios();
        let start = Instant::now();
        policy.on_event(LifecycleEvent::Backgrounded, unpaired, start);
        assert_eq!(
            policy.on_event(
                LifecycleEvent::Foregrounded,
                unpaired,
                start + Duration::from_secs(60)
            ),
            NO_ACTIONS
        );
        assert_eq!(
            policy.on_event(LifecycleEvent::NetworkAvailable, unpaired, start),
            NO_ACTIONS
        );
    }

    #[test]
    fn ios_requests_bounded_time_only_for_in_flight_jobs() {
        let mut policy = ios();
        let start = Instant::now();
        assert_eq!(
            policy.on_event(LifecycleEvent::Backgrounded, PAIRED_CONNECTED, start),
            NO_ACTIONS
        );
        policy.on_event(LifecycleEvent::Foregrounded, PAIRED_CONNECTED, start);

        let busy = AgentFacts {
            pending_jobs: 2,
            ..PAIRED_CONNECTED
        };
        assert_eq!(
            policy.on_event(LifecycleEvent::Backgrounded, busy, start),
            [LifecycleAction::BeginBackgroundTask]
        );
        assert_eq!(
            policy.on_event(LifecycleEvent::JobsDrained, busy, start),
            NO_ACTIONS,
            "the task stays open while jobs remain"
        );
        assert_eq!(
            policy.on_event(LifecycleEvent::JobsDrained, PAIRED_CONNECTED, start),
            [LifecycleAction::EndBackgroundTask]
        );
        assert_eq!(
            policy.on_event(LifecycleEvent::JobsDrained, PAIRED_CONNECTED, start),
            NO_ACTIONS
        );
    }

    #[test]
    fn expired_background_task_is_not_ended_twice() {
        let mut policy = ios();
        let start = Instant::now();
        let busy = AgentFacts {
            pending_jobs: 1,
            ..PAIRED_CONNECTED
        };
        policy.on_event(LifecycleEvent::Backgrounded, busy, start);
        policy.on_event(LifecycleEvent::BackgroundTaskExpired, busy, start);
        let actions = policy.on_event(
            LifecycleEvent::Foregrounded,
            busy,
            start + Duration::from_secs(30),
        );
        assert_eq!(actions, [LifecycleAction::Reconnect]);
    }

    #[test]
    fn android_never_requests_ios_background_tasks() {
        let mut policy = android();
        let busy = AgentFacts {
            pending_jobs: 3,
            ..PAIRED_CONNECTED
        };
        assert_eq!(
            policy.on_event(LifecycleEvent::Backgrounded, busy, Instant::now()),
            NO_ACTIONS
        );
    }

    #[test]
    fn stop_request_shuts_down_and_releases_background_time() {
        let mut policy = ios();
        let start = Instant::now();
        let busy = AgentFacts {
            pending_jobs: 1,
            ..PAIRED_CONNECTED
        };
        policy.on_event(LifecycleEvent::Backgrounded, busy, start);
        assert_eq!(
            policy.on_event(LifecycleEvent::StopRequested, busy, start),
            [
                LifecycleAction::EndBackgroundTask,
                LifecycleAction::Shutdown
            ]
        );
        assert_eq!(
            android().on_event(LifecycleEvent::StopRequested, PAIRED_CONNECTED, start),
            [LifecycleAction::Shutdown]
        );
    }
}
