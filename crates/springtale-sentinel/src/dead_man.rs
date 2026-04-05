use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use tokio::sync::Mutex;

/// Dead-man switch: detects runaway autonomous execution.
///
/// Tracks total actions per minute across all connectors and the last
/// time a user interacted. If too many actions fire without user input,
/// returns a pause verdict.
pub struct DeadManSwitch {
    actions_since_interaction: AtomicU64,
    last_interaction: Mutex<Instant>,
    threshold: u32,
}

impl DeadManSwitch {
    pub fn new(threshold: u32) -> Self {
        Self {
            actions_since_interaction: AtomicU64::new(0),
            last_interaction: Mutex::new(Instant::now()),
            threshold,
        }
    }

    /// Record that an action was dispatched.
    /// Returns `true` if the dead-man switch has triggered (too many actions
    /// without user interaction).
    pub fn record_action(&self) -> bool {
        let count = self
            .actions_since_interaction
            .fetch_add(1, Ordering::SeqCst)
            + 1;
        count > u64::from(self.threshold)
    }

    /// Record that a user sent a message or interacted.
    /// Resets the action counter.
    pub async fn record_user_interaction(&self) {
        self.actions_since_interaction.store(0, Ordering::SeqCst);
        let mut last = self.last_interaction.lock().await;
        *last = Instant::now();
    }

    /// Get the current action count since last interaction.
    pub fn actions_since_last_interaction(&self) -> u64 {
        self.actions_since_interaction.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_under_threshold_allows() {
        let dm = DeadManSwitch::new(5);
        for _ in 0..5 {
            assert!(!dm.record_action());
        }
    }

    #[test]
    fn test_over_threshold_triggers() {
        let dm = DeadManSwitch::new(3);
        assert!(!dm.record_action()); // 1
        assert!(!dm.record_action()); // 2
        assert!(!dm.record_action()); // 3
        assert!(dm.record_action()); // 4 → triggered
    }

    #[tokio::test]
    async fn test_user_interaction_resets() {
        let dm = DeadManSwitch::new(2);
        assert!(!dm.record_action());
        assert!(!dm.record_action());
        assert!(dm.record_action()); // triggered

        dm.record_user_interaction().await;
        assert!(!dm.record_action()); // reset, allowed again
    }

    #[test]
    fn test_counter_tracks() {
        let dm = DeadManSwitch::new(100);
        dm.record_action();
        dm.record_action();
        assert_eq!(dm.actions_since_last_interaction(), 2);
    }
}
