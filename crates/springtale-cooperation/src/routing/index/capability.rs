use std::collections::BinaryHeap;
use std::sync::Mutex;

use dashmap::DashMap;

use super::priority;
use crate::capability::CapabilityDecl;
use crate::routing::types::{PriorityTask, TaskId};

/// Capability-indexed task pool. Each bucket is a priority heap behind a
/// `Mutex` because `BinaryHeap` mutation is not lock-free. The outer
/// `DashMap` keeps bucket lookup contention-free across capabilities.
#[derive(Debug, Default)]
pub struct CapabilityIndex {
    buckets: DashMap<String, Mutex<BinaryHeap<PriorityTask>>>,
}

impl CapabilityIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// File a task under its `target_connector` capability.
    pub fn insert(&self, task: PriorityTask) {
        let cap = task.capability().to_owned();
        let bucket = self.buckets.entry(cap).or_default();
        if let Ok(mut heap) = bucket.lock() {
            heap.push(task);
        }
    }

    /// Return the highest-priority task across the given capability set
    /// without removing it from the index. Caller claims atomically via
    /// `ClaimManager` before taking.
    pub fn peek_best(&self, capabilities: &[CapabilityDecl]) -> Option<PriorityTask> {
        priority::best_across(&self.buckets, capabilities)
    }

    /// Remove a specific task from its bucket (after successful claim+complete).
    pub fn remove(&self, task_id: TaskId, capability: &str) {
        if let Some(bucket) = self.buckets.get(capability)
            && let Ok(mut heap) = bucket.lock()
        {
            let remaining: BinaryHeap<_> =
                heap.drain().filter(|t| t.id() != task_id).collect();
            *heap = remaining;
        }
    }

    pub fn len(&self) -> usize {
        self.buckets
            .iter()
            .map(|b| b.value().lock().map(|h| h.len()).unwrap_or(0))
            .sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::action::SubTask;

    fn task(connector: &str, priority: u8) -> PriorityTask {
        PriorityTask::new(SubTask {
            id: uuid::Uuid::new_v4(),
            target_connector: crate::capability::CapabilityDecl::new(connector),
            action_name: "act".to_owned(),
            params: serde_json::json!({}),
            priority,
            assigned_to: None,
            description: String::new(),
        })
    }

    #[test]
    fn insert_and_peek() {
        let idx = CapabilityIndex::new();
        idx.insert(task("github", 1));
        assert_eq!(idx.len(), 1);
        let best = idx.peek_best(&["github".into()]);
        assert!(best.is_some());
        assert_eq!(best.unwrap().priority(), 1);
    }

    #[test]
    fn peek_returns_highest_priority() {
        let idx = CapabilityIndex::new();
        idx.insert(task("github", 5));
        idx.insert(task("github", 1));
        idx.insert(task("github", 3));
        let best = idx.peek_best(&["github".into()]).unwrap();
        assert_eq!(best.priority(), 1);
    }

    #[test]
    fn peek_across_capabilities() {
        let idx = CapabilityIndex::new();
        idx.insert(task("github", 3));
        idx.insert(task("slack", 1));
        let best = idx.peek_best(&["github".into(), "slack".into()]).unwrap();
        assert_eq!(best.priority(), 1);
    }

    #[test]
    fn non_matching_capability_returns_none() {
        let idx = CapabilityIndex::new();
        idx.insert(task("github", 1));
        assert!(idx.peek_best(&["slack".into()]).is_none());
    }

    #[test]
    fn remove_drops_task_from_bucket() {
        let idx = CapabilityIndex::new();
        let t = task("github", 1);
        let tid = t.id();
        idx.insert(t);
        assert_eq!(idx.len(), 1);
        idx.remove(tid, "github");
        assert_eq!(idx.len(), 0);
    }
}
