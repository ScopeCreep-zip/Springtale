use std::time::Instant;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::action::SubTask;
use crate::cadence::AgentId;

/// Stable identifier for a routable task. Reuses the underlying SubTask uuid.
pub type TaskId = uuid::Uuid;

/// A SubTask wrapped with routing-layer priority data. `Ord` is inverted on
/// `priority` so a `BinaryHeap<PriorityTask>` pops the highest-priority item.
#[derive(Debug, Clone)]
pub struct PriorityTask {
    pub task: SubTask,
}

impl PriorityTask {
    pub fn new(task: SubTask) -> Self {
        Self { task }
    }

    pub fn id(&self) -> TaskId {
        self.task.id
    }

    pub fn capability(&self) -> &str {
        &self.task.target_connector.name
    }

    pub fn priority(&self) -> u8 {
        self.task.priority
    }
}

impl PartialEq for PriorityTask {
    fn eq(&self, other: &Self) -> bool {
        self.task.id == other.task.id && self.task.priority == other.task.priority
    }
}
impl Eq for PriorityTask {}

impl PartialOrd for PriorityTask {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for PriorityTask {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Lower numeric priority = more urgent (priority 1 wins over 5).
        // BinaryHeap is a max-heap, so invert so the smallest u32 pops first.
        other
            .task
            .priority
            .cmp(&self.task.priority)
            .then_with(|| other.task.id.cmp(&self.task.id))
    }
}

/// Ownership record for a claimed task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskClaim {
    pub task_id: TaskId,
    pub owner: AgentId,
    #[serde(skip, default = "Instant::now")]
    pub claimed_at: Instant,
}

#[derive(Debug, Error)]
pub enum RoutingError {
    #[error("another agent already owns task {0}")]
    LostRace(TaskId),
    #[error("task {0} not found in index")]
    NotFound(TaskId),
    #[error("routing substrate unavailable: {0}")]
    Unavailable(String),
}
