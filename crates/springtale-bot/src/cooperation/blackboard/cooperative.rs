//! CooperativeBlackboard — the concrete blackboard for formations.
//!
//! Fuel-metered, capability-scoped, append-only audit log.
//! Per CrewAI/AutoGen/LangGraph patterns: typed entries with
//! schema validation by callers, not raw text passing.

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use uuid::Uuid;

use springtale_cooperation::cadence::AgentId;
use springtale_cooperation::capability::CapabilityDecl;
use springtale_cooperation::{SubTask, SubTaskResult};

use crate::orchestrator::error::OrchestratorError;
use crate::orchestrator::fuel::FuelBudget;

use super::trait_::Blackboard;

/// A single entry on the cooperative blackboard.
#[derive(Debug, Clone)]
pub struct BlackboardEntry {
    pub key: String,
    pub value: serde_json::Value,
    pub written_by: Uuid,
    pub written_at: DateTime<Utc>,
}

/// Audit log entry for blackboard operations.
#[derive(Debug, Clone)]
pub struct BlackboardOp {
    pub op: String,
    pub key: String,
    pub trace_id: Uuid,
    pub timestamp: DateTime<Utc>,
}

/// Cooperative blackboard for formation-level shared state.
///
/// Fuel-metered writes, typed entries, append-only audit log.
/// Per-spawn-group scope (not global).
pub struct CooperativeBlackboard {
    entries: DashMap<String, BlackboardEntry>,
    ops: std::sync::Mutex<Vec<BlackboardOp>>,
}

impl CooperativeBlackboard {
    pub fn new() -> Self {
        Self {
            entries: DashMap::new(),
            ops: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Get the audit log of all operations.
    pub fn audit_log(&self) -> Vec<BlackboardOp> {
        self.ops.lock().map(|ops| ops.clone()).unwrap_or_default()
    }

    fn log_op(&self, op: &str, key: &str, trace_id: Uuid) {
        if let Ok(mut ops) = self.ops.lock() {
            ops.push(BlackboardOp {
                op: op.to_owned(),
                key: key.to_owned(),
                trace_id,
                timestamp: Utc::now(),
            });
        }
    }
}

impl Default for CooperativeBlackboard {
    fn default() -> Self {
        Self::new()
    }
}

impl Blackboard for CooperativeBlackboard {
    fn read(&self, key: &str, reader: Uuid) -> Option<serde_json::Value> {
        let entry = self.entries.get(key)?;
        self.log_op("read", key, reader);
        Some(entry.value.clone())
    }

    fn write(
        &self,
        key: &str,
        value: serde_json::Value,
        writer: Uuid,
        fuel: &FuelBudget,
    ) -> Result<(), OrchestratorError> {
        fuel.consume(1)?;

        let entry = BlackboardEntry {
            key: key.to_owned(),
            value,
            written_by: writer,
            written_at: Utc::now(),
        };
        self.entries.insert(key.to_owned(), entry);
        self.log_op("write", key, writer);
        Ok(())
    }

    fn keys(&self) -> Vec<String> {
        self.entries.iter().map(|e| e.key().clone()).collect()
    }

    fn scan_tasks(&self, agent_capabilities: &[CapabilityDecl]) -> Vec<SubTask> {
        let mut tasks: Vec<SubTask> = self
            .entries
            .iter()
            .filter(|e| e.key().starts_with("task:"))
            .filter_map(|e| serde_json::from_value(e.value.clone()).ok())
            .filter(|task: &SubTask| {
                let claim_key = format!("claimed:{}", task.id);
                !self.entries.contains_key(&claim_key)
            })
            .filter(|task| {
                if agent_capabilities.is_empty() {
                    return true;
                }
                agent_capabilities.contains(&task.target_connector)
            })
            // W3 dependency gate: a task whose upstream results haven't
            // landed yet is invisible to scanners — it becomes claimable
            // the tick after its last dependency posts `result:{id}`.
            .filter(|task: &SubTask| {
                task.depends_on
                    .iter()
                    .all(|dep| self.entries.contains_key(&format!("result:{dep}")))
            })
            .collect();

        tasks.sort_by_key(|t| t.priority);
        tasks
    }

    fn claim_task(
        &self,
        task_id: &str,
        agent_id: AgentId,
        fuel: &FuelBudget,
    ) -> Result<(), OrchestratorError> {
        let claim_key = format!("claimed:{task_id}");
        self.write(
            &claim_key,
            serde_json::json!({ "agent": agent_id.0.to_string() }),
            Uuid::new_v4(),
            fuel,
        )
    }

    fn release_task(&self, task_id: &str) {
        let claim_key = format!("claimed:{task_id}");
        self.entries.remove(&claim_key);
        self.log_op("release", &claim_key, Uuid::new_v4());
    }

    fn post_result(
        &self,
        result: &SubTaskResult,
        fuel: &FuelBudget,
    ) -> Result<(), OrchestratorError> {
        let result_key = format!("result:{}", result.task_id);
        self.write(
            &result_key,
            serde_json::to_value(result).unwrap_or_default(),
            Uuid::new_v4(),
            fuel,
        )?;

        let task_key = format!("task:{}", result.task_id);
        let claim_key = format!("claimed:{}", result.task_id);
        self.entries.remove(&task_key);
        self.entries.remove(&claim_key);

        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn write_and_read_roundtrip() {
        let bb = CooperativeBlackboard::new();
        let fuel = FuelBudget::new(100);
        let trace = Uuid::new_v4();

        bb.write("key1", serde_json::json!("value1"), trace, &fuel)
            .unwrap();
        let val = bb.read("key1", trace).unwrap();
        assert_eq!(val, serde_json::json!("value1"));
    }

    #[test]
    fn read_missing_returns_none() {
        let bb = CooperativeBlackboard::new();
        assert!(bb.read("nonexistent", Uuid::new_v4()).is_none());
    }

    #[test]
    fn write_costs_fuel() {
        let bb = CooperativeBlackboard::new();
        let fuel = FuelBudget::new(2);
        let trace = Uuid::new_v4();

        bb.write("a", serde_json::json!(1), trace, &fuel).unwrap();
        bb.write("b", serde_json::json!(2), trace, &fuel).unwrap();
        assert!(bb.write("c", serde_json::json!(3), trace, &fuel).is_err());
    }

    #[test]
    fn audit_log_records_operations() {
        let bb = CooperativeBlackboard::new();
        let fuel = FuelBudget::new(100);
        let trace = Uuid::new_v4();

        bb.write("key", serde_json::json!("val"), trace, &fuel)
            .unwrap();
        bb.read("key", trace);

        let log = bb.audit_log();
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].op, "write");
        assert_eq!(log[1].op, "read");
    }

    #[test]
    fn keys_lists_all_entries() {
        let bb = CooperativeBlackboard::new();
        let fuel = FuelBudget::new(100);
        let trace = Uuid::new_v4();

        bb.write("alpha", serde_json::json!(1), trace, &fuel)
            .unwrap();
        bb.write("beta", serde_json::json!(2), trace, &fuel)
            .unwrap();

        let keys = bb.keys();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"alpha".to_owned()));
    }

    #[test]
    fn overwrite_replaces_value() {
        let bb = CooperativeBlackboard::new();
        let fuel = FuelBudget::new(100);
        let trace = Uuid::new_v4();

        bb.write("key", serde_json::json!("old"), trace, &fuel)
            .unwrap();
        bb.write("key", serde_json::json!("new"), trace, &fuel)
            .unwrap();

        let val = bb.read("key", trace).unwrap();
        assert_eq!(val, serde_json::json!("new"));
    }
}
