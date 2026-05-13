use serde::{Deserialize, Serialize};
use specta::Type;

use crate::cadence::AgentId;
use crate::routing::types::TaskId;

#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
pub struct Bundle {
    pub owner: AgentId,
    /// Ordered task list — order matters for Diminishing Marginal Gain.
    pub tasks: Vec<TaskId>,
    /// Per-task winning bid values indexed by the outer task order.
    pub bids: Vec<f32>,
    pub iteration: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConvergenceStatus {
    /// Bundle still changing; more gossip rounds needed.
    Running,
    /// No conflicts with any neighbor for N rounds — converged.
    Converged,
    /// Max iterations hit without convergence — escalate.
    Stalled,
}
