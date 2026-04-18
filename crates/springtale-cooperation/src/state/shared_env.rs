//! SharedEnvironment — ArcSwap-backed snapshot layer over the workspace.
//!
//! Per COOPERATION.md §10.3: the formation's shared environment wraps
//! the key-value workspace and surface store behind ArcSwap, so agents
//! read a consistent snapshot (lock-free) and mutations go through RCU.
//!
//! Per LangGraph state management: typed state with reducer merge.
//! Every write produces a new snapshot version; readers always get a
//! consistent point-in-time view.

use std::sync::Arc;

use arc_swap::ArcSwap;
use serde_json::Value;

use crate::cadence::AgentId;
use crate::stigmergy::types::Surface;

use super::snapshot::WorkspaceSnapshot;
use super::trait_::Workspace;
use super::workspace::InMemoryWorkspace;

/// Formation-level shared environment: workspace + surfaces + snapshots.
///
/// Per spec §10.3: `add_surface` composes via reaction table,
/// `write` records to the audit log, and both produce a new snapshot
/// accessible via `snapshot()` (lock-free ArcSwap::load).
pub struct SharedEnvironment {
    workspace: InMemoryWorkspace,
    snapshot: ArcSwap<WorkspaceSnapshot>,
}

impl SharedEnvironment {
    pub fn new() -> Self {
        Self {
            workspace: InMemoryWorkspace::new(),
            snapshot: ArcSwap::from_pointee(WorkspaceSnapshot::default()),
        }
    }

    /// Read a key from the underlying workspace.
    pub fn read(&self, key: &str) -> Option<Value> {
        self.workspace.read(key)
    }

    /// Write a key-value pair and produce a new snapshot.
    pub fn write(&self, key: &str, value: Value, writer: AgentId) {
        self.workspace.write(key.into(), value, writer);
        self.rebuild_snapshot();
    }

    /// Add a surface to the environment and produce a new snapshot.
    pub fn add_surface(&self, surface: Surface) {
        self.snapshot.rcu(|prev| {
            let mut new = (**prev).clone();
            new.surfaces.push(surface.clone());
            new.version += 1;
            Arc::new(new)
        });
    }

    /// Get the current snapshot (lock-free read).
    pub fn snapshot(&self) -> Arc<WorkspaceSnapshot> {
        self.snapshot.load_full()
    }

    /// Current snapshot version.
    pub fn version(&self) -> u64 {
        self.snapshot.load().version
    }

    /// Rebuild the snapshot from the workspace's current state.
    fn rebuild_snapshot(&self) {
        let log = self.workspace.write_log();
        let entries: Vec<(String, Value)> = log
            .iter()
            .filter_map(|(key, _, _)| {
                self.workspace.read(key.as_ref()).map(|v| (key.0.clone(), v))
            })
            .collect();

        self.snapshot.rcu(|prev| {
            let mut new = (**prev).clone();
            new.entries = entries.clone();
            new.version += 1;
            Arc::new(new)
        });
    }
}

impl Default for SharedEnvironment {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for SharedEnvironment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedEnvironment")
            .field("version", &self.version())
            .finish()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::stigmergy::types::{SurfaceId, SurfaceType};
    use std::time::Duration;

    #[test]
    fn write_produces_new_snapshot_version() {
        let env = SharedEnvironment::new();
        let agent = AgentId::new();
        let v0 = env.version();
        env.write("key", serde_json::json!("val"), agent);
        assert!(env.version() > v0);
    }

    #[test]
    fn read_reflects_writes() {
        let env = SharedEnvironment::new();
        let agent = AgentId::new();
        env.write("x", serde_json::json!(42), agent);
        assert_eq!(env.read("x"), Some(serde_json::json!(42)));
    }

    #[test]
    fn snapshot_contains_entries() {
        let env = SharedEnvironment::new();
        let agent = AgentId::new();
        env.write("a", serde_json::json!(1), agent);
        env.write("b", serde_json::json!(2), agent);
        let snap = env.snapshot();
        assert!(snap.entry("a").is_some());
        assert!(snap.entry("b").is_some());
    }

    #[test]
    fn add_surface_appears_in_snapshot() {
        let env = SharedEnvironment::new();
        let agent = AgentId::new();
        let surface = Surface {
            id: SurfaceId::new_v4(),
            created_by: agent,
            surface_type: SurfaceType::Active {
                remaining: Duration::from_secs(10),
            },
            data: serde_json::json!({"type": "alarm"}),
            expires: None,
            capability: None,
        };
        env.add_surface(surface);
        let snap = env.snapshot();
        assert_eq!(snap.surfaces.len(), 1);
    }

    #[test]
    fn snapshot_is_consistent_point_in_time() {
        let env = SharedEnvironment::new();
        let agent = AgentId::new();
        env.write("before", serde_json::json!(true), agent);
        let snap1 = env.snapshot();
        env.write("after", serde_json::json!(true), agent);
        let snap2 = env.snapshot();

        // snap1 shouldn't contain "after"
        assert!(snap1.entry("after").is_none());
        assert!(snap2.entry("after").is_some());
        assert!(snap2.version > snap1.version);
    }
}
