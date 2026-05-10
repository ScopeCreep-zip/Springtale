//! WorkspaceSnapshot — typed read-only view of the shared workspace at a
//! point in time (spec §10).
//!
//! Per COOPERATION.md §10.3: the snapshot carries an ordered audit log of
//! writes (author + timestamp + value), **not** a materialized key-value
//! view. This lets the interference detector run Lamport-ordered negation
//! detection (§13) across ticks and lets replay reconstruct the decision
//! stream deterministically. Callers who need current values call
//! `SharedEnvironment::read` directly.
//!
//! The log is stored as `Arc<Vec<EnvironmentWrite>>` so producing a new
//! snapshot from the prior one is an `Arc::clone` (O(1)) rather than a
//! deep copy — the plan §16.8 bar is 100 ms at 1000 agents and a deep
//! copy per RCU rebuild blew that at ~217 ms.

use std::sync::Arc;

use serde_json::Value;

use crate::cadence::AgentId;
use crate::stigmergy::types::Surface;

use super::workspace::EnvironmentWrite;

/// Immutable snapshot of the shared workspace + active surfaces at a given
/// tick. Produced by the formation-level tick pipeline; consumed by
/// agent-side decision logic and the interference detector.
#[derive(Debug, Clone, Default)]
pub struct WorkspaceSnapshot {
    /// Ordered audit log — oldest first. Used for replay, interference
    /// analysis (§13 ActionNegation), and security audit of cross-agent
    /// workspace modifications.
    pub write_log: Arc<Vec<EnvironmentWrite>>,
    /// Active surfaces (non-expired) at snapshot time.
    pub surfaces: Vec<Surface>,
    /// Monotonic version number — incremented on every write so consumers
    /// can detect staleness without comparing the full log.
    pub version: u64,
}

impl WorkspaceSnapshot {
    pub fn new(write_log: Vec<EnvironmentWrite>, surfaces: Vec<Surface>, version: u64) -> Self {
        Self {
            write_log: Arc::new(write_log),
            surfaces,
            version,
        }
    }

    /// Most recent value for a key as recorded in the log (last-write-wins).
    /// Returns `None` if the key was never written.
    pub fn last_value(&self, key: &str) -> Option<&Value> {
        self.write_log
            .iter()
            .rev()
            .find(|w| w.key.as_ref() == key)
            .map(|w| &w.value)
    }

    /// All distinct writers who have touched a key, in first-write order.
    pub fn writers_of(&self, key: &str) -> Vec<AgentId> {
        let mut seen: Vec<AgentId> = Vec::new();
        for w in self.write_log.iter() {
            if w.key.as_ref() == key && !seen.contains(&w.writer) {
                seen.push(w.writer);
            }
        }
        seen
    }

    /// Surfaces currently in the `Primed` state (ready to react).
    pub fn primed_surfaces(&self) -> Vec<&Surface> {
        self.surfaces
            .iter()
            .filter(|s| {
                matches!(
                    s.surface_type,
                    crate::stigmergy::types::SurfaceType::Primed { .. }
                )
            })
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::types::WorkspaceKey;
    use std::time::Instant;

    fn write(key: &str, value: serde_json::Value, writer: AgentId) -> EnvironmentWrite {
        EnvironmentWrite {
            key: WorkspaceKey::from(key),
            writer,
            value,
            timestamp: Instant::now(),
        }
    }

    #[test]
    fn last_value_returns_most_recent_write() {
        let a = AgentId::new();
        let snap = WorkspaceSnapshot::new(
            vec![
                write("k", serde_json::json!(1), a),
                write("k", serde_json::json!(2), a),
            ],
            vec![],
            2,
        );
        assert_eq!(snap.last_value("k"), Some(&serde_json::json!(2)));
        assert!(snap.last_value("missing").is_none());
    }

    #[test]
    fn writers_of_is_ordered_and_deduplicated() {
        let a = AgentId::new();
        let b = AgentId::new();
        let snap = WorkspaceSnapshot::new(
            vec![
                write("k", serde_json::json!(1), a),
                write("k", serde_json::json!(2), b),
                write("k", serde_json::json!(3), a),
            ],
            vec![],
            3,
        );
        let writers = snap.writers_of("k");
        assert_eq!(writers, vec![a, b]);
    }
}
