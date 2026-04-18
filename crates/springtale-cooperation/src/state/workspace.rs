use std::sync::Mutex;
use std::time::Instant;

use dashmap::DashMap;
use serde_json::Value;

use crate::cadence::AgentId;
use crate::types::WorkspaceKey;

use super::trait_::Workspace;

/// A record of a write operation to the workspace.
pub struct EnvironmentWrite {
    pub key: WorkspaceKey,
    pub writer: AgentId,
    pub timestamp: Instant,
}

/// Concurrent in-memory workspace. Lock-free per-key writes (DashMap) plus
/// a mutexed write-log for audit. Suitable for single-process formations;
/// the distributed variant in Phase 3 will swap behind the `Workspace` trait.
pub struct InMemoryWorkspace {
    cells: DashMap<String, Value>,
    log: Mutex<Vec<EnvironmentWrite>>,
}

impl InMemoryWorkspace {
    pub fn new() -> Self {
        Self {
            cells: DashMap::new(),
            log: Mutex::new(Vec::new()),
        }
    }

    pub fn write_log(&self) -> Vec<(WorkspaceKey, AgentId, Instant)> {
        self.log
            .lock()
            .map(|log| log.iter().map(|e| (e.key.clone(), e.writer, e.timestamp)).collect())
            .unwrap_or_default()
    }
}

impl Default for InMemoryWorkspace {
    fn default() -> Self {
        Self::new()
    }
}

impl Workspace for InMemoryWorkspace {
    fn read(&self, key: &str) -> Option<Value> {
        self.cells.get(key).map(|r| r.value().clone())
    }

    fn write(&self, key: WorkspaceKey, value: Value, writer: AgentId) {
        if let Ok(mut log) = self.log.lock() {
            log.push(EnvironmentWrite {
                key: key.clone(),
                writer,
                timestamp: Instant::now(),
            });
        }
        self.cells.insert(key.0, value);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn read_write_roundtrip() {
        let ws = InMemoryWorkspace::new();
        let agent = AgentId::new();
        ws.write("key".into(), serde_json::json!("value"), agent);
        assert_eq!(ws.read("key"), Some(serde_json::json!("value")));
        assert_eq!(ws.write_log().len(), 1);
    }

    #[test]
    fn log_records_writer_and_key() {
        let ws = InMemoryWorkspace::new();
        let agent = AgentId::new();
        ws.write("a".into(), serde_json::json!(1), agent);
        ws.write("b".into(), serde_json::json!(2), agent);
        let log = ws.write_log();
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].0, *"a");
        assert_eq!(log[1].0, *"b");
        assert_eq!(log[0].1, agent);
    }
}
