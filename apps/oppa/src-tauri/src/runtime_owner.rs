//! Process-wide ownership of the agent runtime.
//!
//! On Android the Rust library outlives activities: the foreground service
//! keeps the process alive while activities are destroyed and recreated. The
//! agent, its connection supervisor, and its job processor must therefore be
//! owned by the process, never by a window or activity. [`RuntimeSlot`]
//! guarantees at most one initialization per process, and [`TaskLease`]
//! guarantees that long-lived tasks such as the connection supervisor are
//! started exactly once even if a host re-runs its setup.

use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use tokio::sync::OnceCell;

/// Single process-wide instance of `T`, created on first use.
pub struct RuntimeSlot<T> {
    cell: OnceCell<Arc<T>>,
}

/// Whether [`RuntimeSlot::acquire`] created the runtime or reused it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ownership {
    /// This call created the runtime.
    Created,
    /// The runtime already existed; for example an Android activity was
    /// recreated while the foreground service kept the process alive.
    Reused,
}

impl<T> RuntimeSlot<T> {
    /// Creates an empty slot usable in a `static`.
    pub const fn new() -> Self {
        Self {
            cell: OnceCell::const_new(),
        }
    }

    /// Returns the existing runtime or initializes it exactly once.
    ///
    /// Concurrent callers wait for the single initializer. A failed
    /// initialization leaves the slot empty so a later attempt may retry.
    pub async fn acquire<F, Fut, E>(&self, initialize: F) -> Result<(Arc<T>, Ownership), E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Arc<T>, E>>,
    {
        let created = AtomicBool::new(false);
        let runtime = self
            .cell
            .get_or_try_init(|| async {
                let runtime = initialize().await?;
                created.store(true, Ordering::Release);
                Ok(runtime)
            })
            .await?;
        let ownership = if created.load(Ordering::Acquire) {
            Ownership::Created
        } else {
            Ownership::Reused
        };
        Ok((Arc::clone(runtime), ownership))
    }

    /// Returns the runtime when it has been initialized.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn get(&self) -> Option<Arc<T>> {
        self.cell.get().cloned()
    }
}

impl<T> Default for RuntimeSlot<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// One-shot permission to start a long-lived task.
#[derive(Debug, Default)]
pub struct TaskLease(AtomicBool);

impl TaskLease {
    /// Returns `true` for exactly one caller over the lease's lifetime.
    pub fn claim(&self) -> bool {
        !self.0.swap(true, Ordering::AcqRel)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_acquisition_creates_one_runtime() {
        let slot = Arc::new(RuntimeSlot::<usize>::new());
        let initializations = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        for _ in 0..16 {
            let slot = Arc::clone(&slot);
            let initializations = Arc::clone(&initializations);
            handles.push(tokio::spawn(async move {
                slot.acquire(|| async {
                    initializations.fetch_add(1, Ordering::SeqCst);
                    tokio::task::yield_now().await;
                    Ok::<_, ()>(Arc::new(7))
                })
                .await
                .expect("runtime")
            }));
        }
        let mut created = 0;
        for handle in handles {
            let (runtime, ownership) = handle.await.expect("task");
            assert_eq!(*runtime, 7);
            if ownership == Ownership::Created {
                created += 1;
            }
        }
        assert_eq!(initializations.load(Ordering::SeqCst), 1);
        assert_eq!(created, 1);
    }

    #[tokio::test]
    async fn recreated_host_reuses_existing_runtime() {
        let slot = RuntimeSlot::<String>::new();
        let (first, ownership) = slot
            .acquire(|| async { Ok::<_, ()>(Arc::new("agent".to_owned())) })
            .await
            .expect("first");
        assert_eq!(ownership, Ownership::Created);
        let (second, ownership) = slot
            .acquire(|| async { Err::<Arc<String>, _>("must not run") })
            .await
            .expect("second");
        assert_eq!(ownership, Ownership::Reused);
        assert!(Arc::ptr_eq(&first, &second));
    }

    #[tokio::test]
    async fn failed_initialization_can_be_retried() {
        let slot = RuntimeSlot::<u8>::new();
        assert!(
            slot.acquire(|| async { Err::<Arc<u8>, _>("disk") })
                .await
                .is_err()
        );
        assert!(slot.get().is_none());
        let (value, ownership) = slot
            .acquire(|| async { Ok::<_, &str>(Arc::new(1)) })
            .await
            .expect("retry");
        assert_eq!((*value, ownership), (1, Ownership::Created));
    }

    #[test]
    fn task_lease_is_claimed_once() {
        let lease = TaskLease::default();
        assert!(lease.claim());
        assert!(!lease.claim());
        assert!(!lease.claim());
    }
}
