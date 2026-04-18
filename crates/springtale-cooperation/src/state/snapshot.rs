//! WorkspaceSnapshot — typed read-only view of the shared workspace at a
//! point in time (spec §10).
//!
//! Surfaces are referenced from `stigmergy::Surface`; workspace entries are
//! materialized from the `Workspace` trait's key-value store. The snapshot
//! enables deterministic replay: save the snapshot, re-feed it, get the same
//! decisions.

use serde_json::Value;

use crate::stigmergy::types::Surface;

/// Immutable snapshot of the shared workspace + active surfaces at a given
/// tick. Produced by the formation-level tick pipeline; consumed by
/// agent-side decision logic that needs a consistent view.
#[derive(Debug, Clone, Default)]
pub struct WorkspaceSnapshot {
    /// Key-value entries visible to the formation at snapshot time.
    pub entries: Vec<(String, Value)>,
    /// Active surfaces (non-expired) at snapshot time.
    pub surfaces: Vec<Surface>,
    /// Monotonic version number — incremented on every write so consumers
    /// can detect staleness without comparing the full entry set.
    pub version: u64,
}

impl WorkspaceSnapshot {
    pub fn new(entries: Vec<(String, Value)>, surfaces: Vec<Surface>, version: u64) -> Self {
        Self {
            entries,
            surfaces,
            version,
        }
    }

    pub fn entry(&self, key: &str) -> Option<&Value> {
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    pub fn primed_surfaces(&self) -> Vec<&Surface> {
        self.surfaces
            .iter()
            .filter(|s| matches!(s.surface_type, crate::stigmergy::types::SurfaceType::Primed { .. }))
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn entry_lookup() {
        let snap = WorkspaceSnapshot::new(
            vec![("key".into(), serde_json::json!("val"))],
            vec![],
            1,
        );
        assert_eq!(snap.entry("key"), Some(&serde_json::json!("val")));
        assert!(snap.entry("missing").is_none());
    }
}
