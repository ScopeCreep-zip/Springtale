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
}
