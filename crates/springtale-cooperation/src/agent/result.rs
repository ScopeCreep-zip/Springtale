//! Per-tick result from one agent's `AgentLoop::tick()`.
//!
//! Canonical home for `AgentTickResult` — the value every `agent/step/*`
//! function returns when it produces a tick action. The `Idle` constructor
//! is the early-exit fall-through used by `AgentLoop::tick` when no step
//! produced a result.

use crate::action::SubTask;
use crate::cadence::{ActionDescriptor, AgentId};

/// Result of one agent's tick — feeds back into the formation tick pipeline.
pub struct AgentTickResult {
    /// Which agent this result is for.
    pub agent_id: AgentId,
    /// What action was taken this tick (for `TickReport.action_taken`).
    /// `None` when the agent was idle or in observe/suggest mode.
    pub action: Option<ActionDescriptor>,
    /// How well aligned the agent's action was with formation intent (0.0-1.0).
    pub alignment: f32,
    /// Agents interfered with (if any).
    pub interference_with: Vec<AgentId>,
    /// Whether a task was claimed this tick.
    pub task_claimed: Option<SubTask>,
    /// Whether a task completed this tick.
    pub task_completed: bool,
}

impl AgentTickResult {
    /// Idle constructor used by `AgentLoop::tick` when no step produced an
    /// action this tick.
    pub fn idle(agent_id: AgentId) -> Self {
        Self {
            agent_id,
            action: None,
            alignment: 1.0,
            interference_with: vec![],
            task_claimed: None,
            task_completed: false,
        }
    }
}
