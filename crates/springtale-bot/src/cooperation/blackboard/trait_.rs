//! Blackboard trait — the formation-level shared workspace interface.
//!
//! Per Hayes-Roth (1985): a blackboard is a shared data structure where
//! knowledge sources post incremental solutions. Our blackboard composes
//! key-value state, task routing, surface sensing, and result collection.

use serde_json::Value;
use uuid::Uuid;

use springtale_cooperation::cadence::AgentId;
use springtale_cooperation::capability::CapabilityDecl;
use springtale_cooperation::{SubTask, SubTaskResult};

use crate::orchestrator::error::OrchestratorError;
use crate::orchestrator::fuel::FuelBudget;

/// Trait for formation-level blackboard implementations.
///
/// Separates the blackboard interface from its implementation so
/// the formation can be tested with mock blackboards.
pub trait Blackboard: Send + Sync {
    fn read(&self, key: &str, reader: Uuid) -> Option<Value>;
    fn write(
        &self,
        key: &str,
        value: Value,
        writer: Uuid,
        fuel: &FuelBudget,
    ) -> Result<(), OrchestratorError>;
    fn keys(&self) -> Vec<String>;
    fn scan_tasks(&self, capabilities: &[CapabilityDecl]) -> Vec<SubTask>;
    fn claim_task(
        &self,
        task_id: &str,
        agent_id: AgentId,
        fuel: &FuelBudget,
    ) -> Result<(), OrchestratorError>;
    fn release_task(&self, task_id: &str);
    fn post_result(
        &self,
        result: &SubTaskResult,
        fuel: &FuelBudget,
    ) -> Result<(), OrchestratorError>;

    /// W3 cross-agent data pipe: read a completed task's result (written by
    /// [`Self::post_result`] under `result:{task_id}`). This is how a
    /// dependent task consumes an upstream member's output — the read half
    /// the pipe was missing. Default rides [`Self::read`] so every impl
    /// (including test mocks) gets it for free.
    fn read_result(&self, task_id: Uuid) -> Option<SubTaskResult> {
        self.read(&format!("result:{task_id}"), Uuid::nil())
            .and_then(|v| serde_json::from_value(v).ok())
    }
}
