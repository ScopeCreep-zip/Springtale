//! Handles to the background tasks one [`RuntimeState`] owns.
//!
//! [`RuntimeState`]: crate::state::RuntimeState
//!
//! Locking the daemon (plan 6.10) drops the `RuntimeState` so the SQLite
//! handle closes and the database key zeroizes. That only works if
//! nothing else is still holding a clone — and `init` plus
//! `bootstrap_embedded` spawn a dozen long-lived tasks that each captured
//! `store`, `registry` or the whole state. Every one of them registers
//! here, so [`TaskHandles::shutdown`] can abort them *and wait for the
//! abort to land* before the caller drops its own clone.
//!
//! Awaiting matters: `JoinHandle::abort` only marks a task for
//! cancellation. The captured `Arc`s are released when the executor
//! actually drops the task, and awaiting the handle is the one
//! synchronisation point that guarantees it happened.

use std::future::Future;
use std::sync::{Arc, Mutex};

use tokio::task::JoinHandle;

/// A shared, cloneable set of spawned background tasks.
///
/// Cheap to clone — every clone refers to the same list, so a task
/// spawned through any clone is aborted by `shutdown` on any other.
#[derive(Clone, Default)]
pub struct TaskHandles {
    inner: Arc<Mutex<Vec<JoinHandle<()>>>>,
}

impl TaskHandles {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an already-spawned task.
    pub fn push(&self, handle: JoinHandle<()>) {
        // A poisoned mutex means some other thread panicked while
        // holding it. The list itself is still structurally sound (a
        // `Vec<JoinHandle>` push cannot leave a torn value), and losing
        // track of a task would defeat the whole point of this type, so
        // recover rather than propagate.
        match self.inner.lock() {
            Ok(mut list) => list.push(handle),
            Err(poisoned) => poisoned.into_inner().push(handle),
        }
    }

    /// `tokio::spawn` the future and register the handle in one step.
    pub fn spawn<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.push(tokio::spawn(future));
    }

    /// How many tasks are currently registered.
    pub fn len(&self) -> usize {
        match self.inner.lock() {
            Ok(list) => list.len(),
            Err(poisoned) => poisoned.into_inner().len(),
        }
    }

    /// Whether no task is registered.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Abort every registered task and wait for each to stop.
    ///
    /// Returns how many were aborted. Idempotent — the list is drained,
    /// so a second call is a no-op.
    pub async fn shutdown(&self) -> usize {
        let handles: Vec<JoinHandle<()>> = match self.inner.lock() {
            Ok(mut list) => std::mem::take(&mut *list),
            Err(poisoned) => std::mem::take(&mut *poisoned.into_inner()),
        };
        for handle in &handles {
            handle.abort();
        }
        let count = handles.len();
        for handle in handles {
            // `Err(JoinError::Cancelled)` is the expected outcome; what
            // matters is that awaiting returns only once the task has
            // been dropped and its captured state released.
            let _ = handle.await;
        }
        count
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_shutdown_aborts_and_releases_captured_state() {
        let tasks = TaskHandles::new();
        let captured = Arc::new(());
        let held = captured.clone();
        tasks.spawn(async move {
            let _keep = held;
            // Never completes — only an abort ends this task.
            std::future::pending::<()>().await;
        });
        assert_eq!(tasks.len(), 1);
        assert_eq!(Arc::strong_count(&captured), 2);

        assert_eq!(tasks.shutdown().await, 1);

        assert!(tasks.is_empty());
        assert_eq!(
            Arc::strong_count(&captured),
            1,
            "aborted task must have released its captured Arc"
        );
    }

    #[tokio::test]
    async fn test_shutdown_is_idempotent() {
        let tasks = TaskHandles::new();
        tasks.spawn(async {});
        assert_eq!(tasks.shutdown().await, 1);
        assert_eq!(tasks.shutdown().await, 0);
    }
}
