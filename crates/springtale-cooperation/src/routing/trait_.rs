use async_trait::async_trait;

use crate::action::SubTaskResult;
use crate::awareness::LocalAwareness;
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
    /// check in the agent tick — assigned work preempts scanning. Implementations
    /// should consult both per-agent direct-handoff inboxes
    /// (`COOPERATION.md §20.1`) and CBBA-style `assigned_to` markers
    /// (`COOPERATION.md §20.2`); the inbox path covers `HandoffType::Direct`
    /// dispatched via `dispatch_handoff_durable`.
    async fn poll_assigned(&self, agent: AgentId) -> Option<PriorityTask>;

    /// L3 work-stealing: try to steal a `FlexibleChain` payload from the
    /// per-capability work-stealing pool (`COOPERATION.md §20.4`). Returns
    /// the highest-priority match across the agent's capabilities. Default
    /// impl returns `None` — connectors that don't expose a flex-chain
    /// substrate (test mocks, simple routers) keep the trait satisfied
    /// without extra wiring. Production `BlackboardRouter` overrides this
    /// to check `FlexibleChainPool::find_task` per capability.
    async fn try_steal_chain(
        &self,
        _capabilities: &[CapabilityDecl],
        _agent: AgentId,
    ) -> Option<PriorityTask> {
        None
    }

    /// L1: scan capability-indexed queues and return the best-priority match
    /// across the agent's `capabilities`. Does not claim — that's a separate
    /// step so scan-without-intent (e.g. "suggest" autonomy) doesn't race.
    ///
    /// At Warming+ tier, implementations may consult `awareness` (peer
    /// TickReports) to boost priority for connectors where neighbors
    /// recently succeeded — the "Total War proximity morale" weighting
    /// from `COOPERATION.md §8`. Cold tier ignores awareness. The default
    /// path is `awareness = None`; bot-side callers pass `Some(&aw)`.
    async fn scan(
        &self,
        capabilities: &[CapabilityDecl],
        tier: MomentumTier,
        awareness: Option<&LocalAwareness>,
    ) -> Option<PriorityTask>;

    /// L1: atomic ownership acquisition. Returns `RoutingError::LostRace`
    /// when another agent already holds this task id.
    async fn claim(&self, task_id: TaskId, agent: AgentId) -> Result<TaskClaim, RoutingError>;

    /// Publish result and release the claim + remove the task from the index.
    async fn complete(&self, task_id: TaskId, result: SubTaskResult);

    /// Release a claim without completion (failure, preemption, cancellation).
    /// Task returns to the index at its original priority.
    async fn release(&self, task_id: TaskId);
}
