use async_trait::async_trait;

use crate::action::SubTaskResult;
use crate::cadence::AgentId;
use crate::capability::CapabilityDecl;
use crate::momentum::MomentumTier;

use super::types::{PriorityTask, RoutingError, TaskClaim, TaskId};

/// Runtime routing interface consumed by the agent loop's L1/L3 steps.
///
/// Implementations compose the per-concern stores (capability index, claim
/// manager, direct inbox) rather than reimplementing routing themselves.
#[async_trait]
pub trait TaskRouter: Send + Sync {
    /// L3: pull a task directly assigned to `agent`, if any. Highest priority
    /// check in the agent tick — assigned work preempts scanning.
    async fn poll_assigned(&self, agent: AgentId) -> Option<PriorityTask>;

    /// L1: scan capability-indexed queues and return the best-priority match
    /// across the agent's `capabilities`. Does not claim — that's a separate
    /// step so scan-without-intent (e.g. "suggest" autonomy) doesn't race.
    async fn scan(&self, capabilities: &[CapabilityDecl], tier: MomentumTier) -> Option<PriorityTask>;

    /// L1: atomic ownership acquisition. Returns `RoutingError::LostRace`
    /// when another agent already holds this task id.
    async fn claim(&self, task_id: TaskId, agent: AgentId) -> Result<TaskClaim, RoutingError>;

    /// Publish result and release the claim + remove the task from the index.
    async fn complete(&self, task_id: TaskId, result: SubTaskResult);

    /// Release a claim without completion (failure, preemption, cancellation).
    /// Task returns to the index at its original priority.
    async fn release(&self, task_id: TaskId);
}
