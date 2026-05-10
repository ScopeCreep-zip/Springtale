use std::sync::{Arc, Mutex};
use std::time::Instant;

use dashmap::DashMap;
use serde_json::Value;

use crate::cadence::AgentId;
use crate::types::WorkspaceKey;

use super::trait_::Workspace;

/// A record of a write operation to the workspace. Per COOPERATION.md §10.3,
/// the snapshot carries an ordered audit log of writes (author + timestamp +
/// value) so interference analysis (§13 ActionNegation via Lamport ordering)
/// and replay can reconstruct causality without re-scanning current state.
#[derive(Debug, Clone)]
pub struct EnvironmentWrite {
    pub key: WorkspaceKey,
    pub writer: AgentId,
    pub value: Value,
    pub timestamp: Instant,
}

/// Concurrent in-memory workspace. Lock-free per-key writes (DashMap) plus
/// a mutexed write-log for audit. Suitable for single-process formations;
/// the Veilid-backed distributed variant (the only Phase 3 deferral) will
/// swap behind the `Workspace` trait. Cross-process writes within a
/// single machine already go through the `SharedEnvironment::cas_write`
/// path backed by the workspace `StorageBackend`.
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

    /// Snapshot the ordered write log. Returns an `Arc<Vec<_>>` so the
    /// clone into a fresh `WorkspaceSnapshot` is O(1) — the plan §16.8
    /// 100 ms/1000-agent bar is exceeded if every RCU rebuild deep-copies
    /// the log, so the snapshot carries it by Arc rather than by value.
    pub fn write_log(&self) -> Arc<Vec<EnvironmentWrite>> {
        self.log
            .lock()
            .map(|log| Arc::new(log.clone()))
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
                value: value.clone(),
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
    fn log_records_writer_key_and_value() {
        let ws = InMemoryWorkspace::new();
        let agent = AgentId::new();
        ws.write("a".into(), serde_json::json!(1), agent);
        ws.write("b".into(), serde_json::json!(2), agent);
        let log = ws.write_log();
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].key, *"a");
        assert_eq!(log[1].key, *"b");
        assert_eq!(log[0].writer, agent);
        assert_eq!(log[0].value, serde_json::json!(1));
        assert_eq!(log[1].value, serde_json::json!(2));
    }
}
