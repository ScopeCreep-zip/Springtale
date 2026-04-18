//! Action lifecycle state machine for agent tasks.
//!
//! Per L4D bot architecture: actions have Init→Requested→Executing→
//! Cancelled→Success/Failure lifecycle. The cancellation protocol is
//! a cooperative contract — the formation requests cancel, the agent
//! cooperates by cleaning up and reporting.
//!
//! Per Spring engine: the command queue front is the current task.
//! When a task completes or is cancelled, the agent scans the
//! blackboard for the next task.

use std::time::Instant;

use crate::action::SubTask;
use crate::cadence::AgentId;

/// Lifecycle state of an agent's current action.
///
/// Per L4D source: transitions are atomic. No behavior code executes
/// between the decision to transition and the state change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionState {
    /// Task claimed from blackboard, not yet started.
    Init,
    /// Formation tick confirmed this task should proceed.
    Requested,
    /// Agent is actively executing (connector call in progress).
    Executing,
    /// Formation requested cancellation (higher-priority task, or
    /// formation dissolving). Agent MUST transition to Success or
    /// Failure after cleanup. Per L4D: "Thinkers will wait on
    /// Cancelled actions to do any necessary cleanup work, so this
    /// can hang your AI if you don't look for it."
    Cancelled,
    /// Completed successfully.
    Success,
    /// Failed during execution.
    Failure(String),
}

impl ActionState {
    /// Whether the action is in a terminal state (Success or Failure).
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Success | Self::Failure(_))
    }

    /// Whether the action is actively running.
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Init | Self::Requested | Self::Executing)
    }
}

/// An agent's current task with lifecycle tracking.
///
/// Per Spring engine: command queue front = current task.
/// Per RimWorld: pawn has one active job at a time.
#[derive(Clone)]
pub struct ActiveTask {
    /// The subtask being executed.
    pub task: SubTask,
    /// Current lifecycle state.
    pub state: ActionState,
    /// When this task was claimed from the blackboard.
    pub claimed_at: Instant,
    /// Tick number when claimed (for timeout checking).
    pub claimed_tick: u64,
    /// Which agent claimed this task.
    pub claimed_by: AgentId,
}

impl ActiveTask {
    /// Create a new active task in Init state.
    pub fn new(task: SubTask, agent_id: AgentId, tick: u64) -> Self {
        Self {
            task,
            state: ActionState::Init,
            claimed_at: Instant::now(),
            claimed_tick: tick,
            claimed_by: agent_id,
        }
    }

    /// Advance from Init to Requested.
    pub fn request(&mut self) {
        if self.state == ActionState::Init {
            self.state = ActionState::Requested;
        }
    }

    /// Advance from Requested to Executing.
    pub fn begin_execution(&mut self) {
        if self.state == ActionState::Requested {
            self.state = ActionState::Executing;
        }
    }

    /// Request cancellation. Agent must still clean up and report.
    pub fn cancel(&mut self) {
        if self.state.is_active() {
            self.state = ActionState::Cancelled;
        }
    }

    /// Mark as successfully completed.
    pub fn succeed(&mut self) {
        self.state = ActionState::Success;
    }

    /// Mark as failed.
    pub fn fail(&mut self, reason: String) {
        self.state = ActionState::Failure(reason);
    }

    /// Check if this task has been active longer than the given tick count.
    pub fn ticks_elapsed(&self, current_tick: u64) -> u64 {
        current_tick.saturating_sub(self.claimed_tick)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::cadence::AgentId;

    fn make_task() -> SubTask {
        SubTask {
            id: uuid::Uuid::new_v4(),
            target_connector: crate::capability::CapabilityDecl::new("connector-github"),
            action_name: "create_issue".to_owned(),
            params: serde_json::json!({}),
            priority: 1,
            assigned_to: None,
            description: "test task".to_owned(),
        }
    }

    #[test]
    fn test_full_lifecycle_success() {
        let agent = AgentId::new();
        let mut active = ActiveTask::new(make_task(), agent, 0);
        assert_eq!(active.state, ActionState::Init);

        active.request();
        assert_eq!(active.state, ActionState::Requested);

        active.begin_execution();
        assert_eq!(active.state, ActionState::Executing);

        active.succeed();
        assert_eq!(active.state, ActionState::Success);
        assert!(active.state.is_terminal());
    }

    #[test]
    fn test_full_lifecycle_failure() {
        let agent = AgentId::new();
        let mut active = ActiveTask::new(make_task(), agent, 0);
        active.request();
        active.begin_execution();
        active.fail("connector timeout".to_owned());
        assert!(matches!(active.state, ActionState::Failure(ref r) if r == "connector timeout"));
        assert!(active.state.is_terminal());
    }

    #[test]
    fn test_cancellation() {
        let agent = AgentId::new();
        let mut active = ActiveTask::new(make_task(), agent, 0);
        active.request();
        active.begin_execution();
        active.cancel();
        assert_eq!(active.state, ActionState::Cancelled);
        assert!(!active.state.is_active());
        assert!(!active.state.is_terminal());

        // Agent cooperates by reporting failure after cleanup
        active.fail("cancelled by formation".to_owned());
        assert!(active.state.is_terminal());
    }

    #[test]
    fn test_ticks_elapsed() {
        let agent = AgentId::new();
        let active = ActiveTask::new(make_task(), agent, 10);
        assert_eq!(active.ticks_elapsed(15), 5);
        assert_eq!(active.ticks_elapsed(10), 0);
    }

    #[test]
    fn test_cannot_cancel_terminal() {
        let agent = AgentId::new();
        let mut active = ActiveTask::new(make_task(), agent, 0);
        active.succeed();
        active.cancel(); // should have no effect
        assert_eq!(active.state, ActionState::Success);
    }
}
