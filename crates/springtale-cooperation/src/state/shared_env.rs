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
use crate::error::CooperationError;
use crate::interference::{self, InterferenceEvent};
use crate::stigmergy::composition::compose_surfaces;
use crate::stigmergy::composition::table::ReactionTable;
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
    reaction_table: ReactionTable,
}

impl SharedEnvironment {
    pub fn new() -> Self {
        Self::with_reaction_table(ReactionTable::cooperation_defaults())
    }

    /// Construct with a custom reaction table. Used by tests that want to
    /// exercise `add_surface` with a minimal or alternative vocabulary.
    pub fn with_reaction_table(reaction_table: ReactionTable) -> Self {
        Self {
            workspace: InMemoryWorkspace::new(),
            snapshot: ArcSwap::from_pointee(WorkspaceSnapshot::default()),
            reaction_table,
        }
    }

    /// Read a key from the underlying workspace.
    pub fn read(&self, key: &str) -> Option<Value> {
        self.workspace.read(key)
    }

    /// Write a key-value pair and produce a new snapshot.
    ///
    /// This is the fast-path write that skips cross-agent CAS. Use it
    /// only for formation-internal bookkeeping where concurrent-writer
    /// detection isn't meaningful (e.g. the supervisor annotating its
    /// own state). Cross-agent writes should go through
    /// [`cas_write`](Self::cas_write) so `§13` interference detection
    /// can classify conflicts.
    pub fn write(&self, key: &str, value: Value, writer: AgentId) {
        self.workspace.write(key.into(), value, writer);
        self.rebuild_snapshot();
    }

    /// CAS-gated write routed through the workspace store. Per
    /// COOPERATION.md §13.2, concurrent writes to the same key from
    /// different agents produce `InterferenceEvent`s (ResourceConflict
    /// for distinct values, Redundancy for same-value overlap) rather
    /// than silent last-write-wins.
    ///
    /// On success (no conflict), the value is applied to the local
    /// workspace and a fresh snapshot is produced. On mismatch, returns
    /// the conflict event WITHOUT applying the proposed write — the
    /// caller decides whether to retry or yield.
    ///
    /// `tick` identifies the current tick sequence for event attribution.
    /// `expected` is the value the writer believed was present (None for
    /// "expected absent"); the backend's atomic compare-and-swap uses
    /// this for mismatch classification.
    pub async fn cas_write(
        &self,
        store: &Arc<dyn springtale_store::StorageBackend>,
        tick: crate::tick::TickId,
        writer: AgentId,
        key: &str,
        expected: Option<&[u8]>,
        value: Value,
    ) -> Result<Option<InterferenceEvent>, CooperationError> {
        let proposed_bytes = serde_json::to_vec(&value)
            .map_err(|e| CooperationError::Invariant(format!("serialize cas value: {e}")))?;
        let event =
            interference::cas_apply(store, tick, writer, key, expected, &proposed_bytes).await?;
        if event.is_none() {
            // CAS succeeded — mirror the write into the in-process
            // workspace so readers see the new value and the local
            // write_log has the audit entry for ActionNegation.
            self.workspace.write(key.into(), value, writer);
            self.rebuild_snapshot();
        }
        Ok(event)
    }

    /// Add a surface to the environment. Per spec §10.3, the incoming
    /// surface is composed with the existing surfaces via the reaction
    /// table before the snapshot is updated. This is where elemental-style
    /// reactions (Noita / DOS2 / CDDA) fire for the cooperation vocabulary
    /// (see `stigmergy::composition::table::tags`).
    pub fn add_surface(&self, surface: Surface) {
        let author = surface.created_by;
        self.snapshot.rcu(|prev| {
            let result = compose_surfaces(&prev.surfaces, &surface, &self.reaction_table, author);
            let mut next = (**prev).clone();
            let mut merged = result.surviving.clone();
            merged.extend(result.spawned.clone());
            next.surfaces = merged;
            next.version += 1;
            Arc::new(next)
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

    /// Rebuild the snapshot from the workspace's current write log.
    /// Per spec §10.3 the snapshot carries the ordered audit trail, not
    /// a materialized key-value view — callers needing current values
    /// hit `read()` directly.
    ///
    /// Previously cloned the prior snapshot in full (`(**prev).clone()`)
    /// before overwriting `write_log`, which made each write O(N) in
    /// the log length and produced an O(N²) burst under 1000 writes
    /// (plan §16.8). We now build the new snapshot from the fresh log
    /// alongside the prior `surfaces` directly, which saves a full log
    /// clone per RCU attempt.
    fn rebuild_snapshot(&self) {
        let log = self.workspace.write_log();
        self.snapshot.rcu(|prev| {
            Arc::new(WorkspaceSnapshot {
                // O(1) Arc clone — the underlying Vec is shared across
                // the workspace and every concurrent snapshot holder.
                write_log: Arc::clone(&log),
                surfaces: prev.surfaces.clone(),
                version: prev.version + 1,
            })
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
    fn snapshot_contains_write_log_entries() {
        let env = SharedEnvironment::new();
        let agent = AgentId::new();
        env.write("a", serde_json::json!(1), agent);
        env.write("b", serde_json::json!(2), agent);
        let snap = env.snapshot();
        assert_eq!(snap.last_value("a"), Some(&serde_json::json!(1)));
        assert_eq!(snap.last_value("b"), Some(&serde_json::json!(2)));
        assert_eq!(snap.writers_of("a"), vec![agent]);
    }

    #[tokio::test]
    async fn cas_write_applies_on_first_write() {
        use springtale_store::StorageBackend;
        use springtale_store::backend::InMemoryBackend;
        let store: std::sync::Arc<dyn StorageBackend> = std::sync::Arc::new(InMemoryBackend::new());
        let env = SharedEnvironment::new();
        let agent = AgentId::new();
        let conflict = env
            .cas_write(
                &store,
                crate::tick::TickId(1),
                agent,
                "k",
                None,
                serde_json::json!("v1"),
            )
            .await
            .unwrap();
        assert!(conflict.is_none());
        // Snapshot reflects the write via the local write_log mirror.
        let snap = env.snapshot();
        assert_eq!(snap.last_value("k"), Some(&serde_json::json!("v1")));
    }

    #[tokio::test]
    async fn cas_write_detects_resource_conflict() {
        use crate::interference::InterferenceType;
        use springtale_store::StorageBackend;
        use springtale_store::backend::InMemoryBackend;
        let store: std::sync::Arc<dyn StorageBackend> = std::sync::Arc::new(InMemoryBackend::new());
        let env = SharedEnvironment::new();
        let a = AgentId::new();
        let b = AgentId::new();
        env.cas_write(
            &store,
            crate::tick::TickId(1),
            a,
            "k",
            None,
            serde_json::json!("v1"),
        )
        .await
        .unwrap();
        // b expects the key absent but a already wrote — mismatch.
        let conflict = env
            .cas_write(
                &store,
                crate::tick::TickId(2),
                b,
                "k",
                None,
                serde_json::json!("v2"),
            )
            .await
            .unwrap()
            .expect("should detect conflict");
        assert!(matches!(
            conflict.interference_type,
            InterferenceType::ResourceConflict
        ));
        // b's write was NOT applied (local mirror still shows v1).
        assert_eq!(
            env.snapshot().last_value("k"),
            Some(&serde_json::json!("v1"))
        );
    }

    #[test]
    fn add_surface_appears_in_snapshot() {
        // Use an empty reaction table so the surface survives without
        // composition consuming it.
        let env = SharedEnvironment::with_reaction_table(
            crate::stigmergy::composition::table::ReactionTable::new(),
        );
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
    fn add_surface_composes_via_reaction_table() {
        use crate::stigmergy::composition::table::tags::*;
        let env = SharedEnvironment::new();
        let agent = AgentId::new();
        // First surface: fresh_input.
        env.add_surface(Surface {
            id: SurfaceId::new_v4(),
            created_by: agent,
            surface_type: SurfaceType::Primed {
                trigger: crate::cadence::ActionDescriptor {
                    kind: FRESH_INPUT.to_owned(),
                    target: None,
                    payload_hash: 0,
                },
            },
            data: serde_json::json!({}),
            expires: None,
            capability: None,
        });
        // Second surface: high_attention → should compose to urgent_response,
        // consuming both.
        env.add_surface(Surface {
            id: SurfaceId::new_v4(),
            created_by: agent,
            surface_type: SurfaceType::Primed {
                trigger: crate::cadence::ActionDescriptor {
                    kind: HIGH_ATTENTION.to_owned(),
                    target: None,
                    payload_hash: 0,
                },
            },
            data: serde_json::json!({}),
            expires: None,
            capability: None,
        });
        let snap = env.snapshot();
        // fresh_input + high_attention → urgent_response (spawned as Active)
        assert_eq!(snap.surfaces.len(), 1);
        let s = &snap.surfaces[0];
        assert!(matches!(s.surface_type, SurfaceType::Active { .. }));
        assert_eq!(s.data["origin"], URGENT_RESPONSE);
    }

    #[test]
    fn snapshot_is_consistent_point_in_time() {
        let env = SharedEnvironment::new();
        let agent = AgentId::new();
        env.write("before", serde_json::json!(true), agent);
        let snap1 = env.snapshot();
        env.write("after", serde_json::json!(true), agent);
        let snap2 = env.snapshot();

        // snap1's write_log shouldn't mention "after"
        assert!(snap1.last_value("after").is_none());
        assert!(snap2.last_value("after").is_some());
        assert!(snap2.version > snap1.version);
    }
}
