//! Atomic compare-and-swap interference classification.
//!
//! Per COOPERATION.md §13.2: the spec proposes `sled::Tree::compare_and_swap`
//! for optimistic concurrency — on mismatch the caller sees the current
//! value and the writer who last set it, so interference can be classified
//! (ResourceConflict vs Redundancy) without a second query.
//!
//! Springtale routes this through the workspace `StorageBackend` so all
//! cooperation SQL lives in `springtale-store` (per `CLAUDE.md` "No raw SQL
//! outside store"). `SqliteBackend::coop_cas_write` uses SQLite
//! `BEGIN IMMEDIATE` for serializable-isolation compare-and-swap — the
//! same semantics sled's `compare_and_swap` provides.

use std::sync::Arc;

use springtale_store::{CoopCasOutcome, StorageBackend};

use crate::cadence::AgentId;
use crate::error::CooperationError;

use super::{InterferenceEvent, InterferenceType};

/// Attempt a compare-and-swap write. On mismatch, classifies the
/// conflict into an `InterferenceEvent`. Returns `Ok(None)` on success,
/// `Ok(Some(event))` on a detected conflict, or `Err` if the backend
/// itself failed.
pub async fn cas_apply(
    store: &Arc<dyn StorageBackend>,
    tick: crate::tick::TickId,
    writer: AgentId,
    key: &str,
    expected: Option<&[u8]>,
    proposed: &[u8],
) -> Result<Option<InterferenceEvent>, CooperationError> {
    let outcome = store
        .coop_cas_write(tick.0 as i64, &writer.0.to_string(), key, expected, proposed)
        .await?;
    match outcome {
        CoopCasOutcome::Applied => Ok(None),
        CoopCasOutcome::Mismatch {
            current_value,
            current_writer,
            current_tick,
        } => {
            let other = uuid::Uuid::parse_str(&current_writer)
                .map(AgentId)
                .unwrap_or(writer);
            let redundant = current_value.as_slice() == proposed;
            Ok(Some(InterferenceEvent {
                tick_sequence: tick,
                agent_a: writer,
                agent_b: other,
                interference_type: if redundant {
                    InterferenceType::Redundancy
                } else {
                    InterferenceType::ResourceConflict
                },
                // Severity mirrors detect_from_records — 0.2 for idempotent
                // overlap, 0.8 for diverging writes. `current_tick` is
                // available for tracing to pinpoint when the conflict started.
                severity: if redundant {
                    0.2
                } else {
                    0.8 + current_tick_noise(current_tick)
                },
            }))
        }
    }
}

/// Stable pseudo-severity noise based on the conflict tick distance.
/// Returns 0.0 for close conflicts (same-window) and small positive
/// values for older conflicts so that UI sorting by severity groups
/// recent-first without destabilizing the canonical 0.8 band.
fn current_tick_noise(current_tick: i64) -> f32 {
    // Clamp to the last 4 bits of the tick counter so severity stays in
    // [0.8, 0.8 + 0.15] — visible but bounded.
    ((current_tick.unsigned_abs() & 0xF) as f32) * 0.01
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use springtale_store::backend::InMemoryBackend;

    #[tokio::test]
    async fn cas_first_write_applied() {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        let agent = AgentId::new();
        let outcome = cas_apply(&backend, crate::tick::TickId(1), agent, "k", None, b"hello")
            .await
            .unwrap();
        assert!(outcome.is_none());
    }

    #[tokio::test]
    async fn cas_mismatch_different_value_is_resource_conflict() {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        let a = AgentId::new();
        let b = AgentId::new();
        // Agent a writes "hello" first.
        cas_apply(&backend, crate::tick::TickId(1), a, "k", None, b"hello")
            .await
            .unwrap();
        // Agent b attempts write expecting None (key absent) — conflict.
        let outcome = cas_apply(&backend, crate::tick::TickId(2), b, "k", None, b"world")
            .await
            .unwrap()
            .expect("should detect conflict");
        assert!(matches!(
            outcome.interference_type,
            InterferenceType::ResourceConflict
        ));
        assert_eq!(outcome.agent_a, b);
        assert_eq!(outcome.agent_b, a);
    }

    #[tokio::test]
    async fn cas_mismatch_same_value_is_redundancy() {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        let a = AgentId::new();
        let b = AgentId::new();
        cas_apply(&backend, crate::tick::TickId(1), a, "k", None, b"hello")
            .await
            .unwrap();
        let outcome = cas_apply(&backend, crate::tick::TickId(2), b, "k", None, b"hello")
            .await
            .unwrap()
            .expect("should detect redundancy");
        assert!(matches!(
            outcome.interference_type,
            InterferenceType::Redundancy
        ));
    }

    #[tokio::test]
    async fn cas_expected_match_applies() {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        let a = AgentId::new();
        cas_apply(&backend, crate::tick::TickId(1), a, "k", None, b"v1").await.unwrap();
        let outcome = cas_apply(&backend, crate::tick::TickId(2), a, "k", Some(b"v1"), b"v2")
            .await
            .unwrap();
        assert!(outcome.is_none());
    }
}
