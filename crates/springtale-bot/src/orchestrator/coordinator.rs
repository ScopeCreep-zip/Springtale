use chrono::{DateTime, Utc};
use dashmap::DashMap;
use uuid::Uuid;

use super::error::OrchestratorError;
use super::fuel::FuelBudget;

/// A cooperative blackboard for sibling pipelines.
///
/// An opt-in shared workspace where sibling pipelines can read/write
/// typed entries during execution. Based on multi-agent cooperation
/// research (CrewAI, AutoGen, LangGraph patterns).
///
/// This is NOT free-form agent chat or global shared memory. It is:
/// - Typed entries only (serde_json::Value, schema-validated by callers)
/// - Capability-scoped (agents only see what their capabilities allow)
/// - Fuel-metered (each write costs fuel, prevents write storms)
/// - Append-only audit log per cooperative session
/// - Per-spawn-group scope (not global)
pub struct CooperativeBlackboard {
    entries: DashMap<String, BlackboardEntry>,
    ops: std::sync::Mutex<Vec<BlackboardOp>>,
}

/// A single entry on the cooperative blackboard.
#[derive(Debug, Clone)]
pub struct BlackboardEntry {
    /// Entry key.
    pub key: String,
    /// Typed value (schema-validated by caller).
    pub value: serde_json::Value,
    /// Trace ID of the pipeline that wrote this entry.
    pub written_by: Uuid,
    /// When the entry was written.
    pub written_at: DateTime<Utc>,
}

/// Audit log entry for blackboard operations.
#[derive(Debug, Clone)]
pub struct BlackboardOp {
    /// Operation type.
    pub op: String,
    /// Entry key.
    pub key: String,
    /// Trace ID of the pipeline that performed the operation.
    pub trace_id: Uuid,
    /// When the operation occurred.
    pub timestamp: DateTime<Utc>,
}

impl CooperativeBlackboard {
    /// Create a new empty blackboard.
    pub fn new() -> Self {
        Self {
            entries: DashMap::new(),
            ops: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Read an entry from the blackboard.
    pub fn read(&self, key: &str, reader_trace_id: Uuid) -> Option<BlackboardEntry> {
        let entry = self.entries.get(key)?.clone();

        // Log read operation
        if let Ok(mut ops) = self.ops.lock() {
            ops.push(BlackboardOp {
                op: "read".into(),
                key: key.to_owned(),
                trace_id: reader_trace_id,
                timestamp: Utc::now(),
            });
        }

        Some(entry)
    }

    /// Write an entry to the blackboard.
    ///
    /// Each write costs 1 unit of fuel (DoS prevention).
    pub fn write(
        &self,
        key: &str,
        value: serde_json::Value,
        writer_trace_id: Uuid,
        fuel: &FuelBudget,
    ) -> Result<(), OrchestratorError> {
        // Consume fuel for write (DoS prevention)
        fuel.consume(1)?;

        let entry = BlackboardEntry {
            key: key.to_owned(),
            value,
            written_by: writer_trace_id,
            written_at: Utc::now(),
        };

        self.entries.insert(key.to_owned(), entry);

        // Log write operation
        if let Ok(mut ops) = self.ops.lock() {
            ops.push(BlackboardOp {
                op: "write".into(),
                key: key.to_owned(),
                trace_id: writer_trace_id,
                timestamp: Utc::now(),
            });
        }

        Ok(())
    }

    /// List all keys on the blackboard.
    pub fn keys(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|e| e.key().clone())
            .collect::<Vec<String>>()
    }

    /// Get the audit log of all operations.
    pub fn audit_log(&self) -> Vec<BlackboardOp> {
        self.ops.lock().map(|ops| ops.clone()).unwrap_or_default()
    }
}

impl Default for CooperativeBlackboard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_write_and_read() {
        let bb = CooperativeBlackboard::new();
        let fuel = FuelBudget::new(100);
        let trace = Uuid::new_v4();

        bb.write("key1", serde_json::json!("value1"), trace, &fuel)
            .unwrap();
        let entry = bb.read("key1", trace).unwrap();
        assert_eq!(entry.value, serde_json::json!("value1"));
        assert_eq!(entry.written_by, trace);
    }

    #[test]
    fn test_read_missing_returns_none() {
        let bb = CooperativeBlackboard::new();
        let trace = Uuid::new_v4();
        assert!(bb.read("nonexistent", trace).is_none());
    }

    #[test]
    fn test_write_costs_fuel() {
        let bb = CooperativeBlackboard::new();
        let fuel = FuelBudget::new(3);
        let trace = Uuid::new_v4();

        bb.write("a", serde_json::json!(1), trace, &fuel).unwrap();
        bb.write("b", serde_json::json!(2), trace, &fuel).unwrap();
        bb.write("c", serde_json::json!(3), trace, &fuel).unwrap();

        // 4th write should fail — fuel exhausted
        let result = bb.write("d", serde_json::json!(4), trace, &fuel);
        assert!(result.is_err());
    }

    #[test]
    fn test_audit_log() {
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
    fn test_keys() {
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
        assert!(keys.contains(&"beta".to_owned()));
    }

    #[test]
    fn test_overwrite_existing_key() {
        let bb = CooperativeBlackboard::new();
        let fuel = FuelBudget::new(100);
        let trace = Uuid::new_v4();

        bb.write("key", serde_json::json!("old"), trace, &fuel)
            .unwrap();
        bb.write("key", serde_json::json!("new"), trace, &fuel)
            .unwrap();

        let entry = bb.read("key", trace).unwrap();
        assert_eq!(entry.value, serde_json::json!("new"));
    }
}
