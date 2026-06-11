//! Subtask types for formation orchestration.
//!
//! When the formation orchestrator decomposes an intent into work,
//! it posts `SubTask`s to the CooperativeBlackboard. Members pull
//! tasks matching their capabilities (RimWorld work priority pattern).
//! Results are reported back via `SubTaskResult`.

use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

use crate::cadence::AgentId;
use crate::capability::CapabilityDecl;

/// A subtask posted to the blackboard by the orchestrator.
///
/// Members pull subtasks matching their connector capabilities.
/// The `assigned_to` field is a hint (role bias per §23),
/// not a mandate — any capable member can pick up the task.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SubTask {
    /// Unique task identifier.
    pub id: Uuid,
    /// Target connector capability (e.g., "connector-github").
    pub target_connector: CapabilityDecl,
    /// Action to execute (e.g., "create_issue").
    pub action_name: String,
    /// Action parameters (JSON).
    pub params: serde_json::Value,
    /// Priority (1 = highest). Members check highest priority first.
    pub priority: u8,
    /// Suggested agent — a hint, not a lock (§23 bias not mandate).
    pub assigned_to: Option<AgentId>,
    /// Human-readable description for the UI.
    pub description: String,
    /// W3 cross-agent data pipe: ids of tasks whose results this task
    /// consumes. A task with unmet dependencies stays unclaimed; on claim,
    /// `${result:<uuid>...}` placeholders in `params` resolve from the
    /// dependencies' `result:*` blackboard entries. Empty = independent
    /// (the pre-W3 behavior, and the serde default for stored tasks).
    #[serde(default)]
    pub depends_on: Vec<Uuid>,
}

/// Result of a member executing a subtask.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SubTaskResult {
    /// Which task was executed.
    pub task_id: Uuid,
    /// Which agent executed it.
    pub agent_id: AgentId,
    /// Whether execution succeeded.
    pub success: bool,
    /// Action output (connector response).
    pub output: serde_json::Value,
    /// Execution duration in milliseconds.
    pub duration_ms: u64,
}
