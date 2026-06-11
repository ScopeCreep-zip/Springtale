//! `InMemoryKnowledgeStore` — DashMap-backed default.
//!
//! Tag-overlap relevance scoring is the v1 ranker:
//!   - `intent_match` = 1.0 if the query and note carry the same
//!     `IntentPattern` variant (`Execute` vs `Reconnoiter` vs etc.), 0.0
//!     otherwise.
//!   - `connector_overlap` = `|query.connectors ∩ note.connectors| /
//!     max(|query|, |note|, 1)`. Jaccard-style, capped to 1.0.
//!   - `score = 0.6 * intent_match + 0.4 * connector_overlap`.
//!
//! These weights map directly to the "intent first, capability second"
//! heuristic the orchestrator already uses — a future vector-similarity
//! backend will replace the body of `score_against` without touching the
//! call sites in `lifecycle::spawn_formation`.

use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;

use crate::types::FormationId;

use super::trait_::GlobalKnowledgeStore;
use super::types::{OutcomeNote, PriorOutcome, RetrievalQuery};

pub struct InMemoryKnowledgeStore {
    notes: DashMap<FormationId, OutcomeNote>,
}

impl InMemoryKnowledgeStore {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            notes: DashMap::new(),
        })
    }
}

impl Default for InMemoryKnowledgeStore {
    fn default() -> Self {
        Self {
            notes: DashMap::new(),
        }
    }
}

#[async_trait]
impl GlobalKnowledgeStore for InMemoryKnowledgeStore {
    async fn record_outcome(&self, note: OutcomeNote) {
        self.notes.insert(note.formation_id, note);
    }

    async fn retrieve_relevant(&self, query: &RetrievalQuery, k: usize) -> Vec<PriorOutcome> {
        if k == 0 {
            return Vec::new();
        }
        let mut scored: Vec<PriorOutcome> = self
            .notes
            .iter()
            .map(|entry| {
                let note = entry.value().clone();
                let score = score_against(query, &note);
                PriorOutcome { note, score }
            })
            .filter(|o| o.score > 0.0)
            .collect();
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(k);
        scored
    }

    async fn len(&self) -> usize {
        self.notes.len()
    }
}

/// Relevance scorer — exposed at module scope so the persistent backend
/// can share the same ranking logic. A future vector backend will
/// override by ignoring this function entirely.
pub(super) fn score_against(query: &RetrievalQuery, note: &OutcomeNote) -> f32 {
    let intent_match = if intent_variant_eq(&query.intent, &note.intent) {
        1.0
    } else {
        0.0
    };
    let q_caps: std::collections::HashSet<&str> =
        query.connectors.iter().map(String::as_str).collect();
    let n_caps: std::collections::HashSet<&str> =
        note.connectors.iter().map(String::as_str).collect();
    let intersect = q_caps.intersection(&n_caps).count() as f32;
    let denom = q_caps.len().max(n_caps.len()).max(1) as f32;
    let connector_overlap = intersect / denom;
    0.6 * intent_match + 0.4 * connector_overlap
}

/// Variant-only comparison — `IntentPattern` carries payloads (`plan_id`,
/// `reason`) that differ between formations even when the intent
/// category is the same. Ranking should treat `Execute { plan_id: A }`
/// and `Execute { plan_id: B }` as same-intent for retrieval purposes.
fn intent_variant_eq(a: &crate::cadence::IntentPattern, b: &crate::cadence::IntentPattern) -> bool {
    use crate::cadence::IntentPattern as I;
    matches!(
        (a, b),
        (I::Execute { .. }, I::Execute { .. })
            | (I::Reconnoiter { .. }, I::Reconnoiter { .. })
            | (I::Stabilize { .. }, I::Stabilize { .. })
            | (I::Surge { .. }, I::Surge { .. })
            | (I::Dissolve { .. }, I::Dissolve { .. })
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::cadence::IntentPattern;
    use crate::momentum::MomentumTier;

    fn note(connectors: &[&str], intent: IntentPattern) -> OutcomeNote {
        OutcomeNote {
            formation_id: FormationId::new(),
            intent,
            peak_tier: MomentumTier::Hot,
            connectors: connectors.iter().map(|s| (*s).into()).collect(),
            success_count: 3,
            failure_count: 0,
            dissolve_reason: "complete".into(),
            at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn empty_store_returns_nothing() {
        let store = InMemoryKnowledgeStore::new();
        let q = RetrievalQuery {
            intent: IntentPattern::Execute { plan_id: None },
            connectors: vec!["slack".into()],
        };
        assert!(store.retrieve_relevant(&q, 5).await.is_empty());
        assert!(store.is_empty().await);
    }

    #[tokio::test]
    async fn ranks_by_intent_then_connector_overlap() {
        let store = InMemoryKnowledgeStore::new();
        store
            .record_outcome(note(
                &["slack", "github"],
                IntentPattern::Execute { plan_id: None },
            ))
            .await;
        store
            .record_outcome(note(
                &["nostr"],
                IntentPattern::Reconnoiter {
                    target: crate::cadence::TaskDescriptor("scan".into()),
                },
            ))
            .await;
        store
            .record_outcome(note(&["slack"], IntentPattern::Execute { plan_id: None }))
            .await;
        let q = RetrievalQuery {
            intent: IntentPattern::Execute { plan_id: None },
            connectors: vec!["slack".into(), "github".into()],
        };
        let out = store.retrieve_relevant(&q, 3).await;
        // First note has full intent match + full connector overlap.
        assert_eq!(out.len(), 2, "Reconnoiter intent filtered (score 0)");
        assert!(out[0].score >= out[1].score);
        assert!(out[0].note.connectors.contains(&"github".to_owned()));
    }

    #[tokio::test]
    async fn k_zero_is_empty() {
        let store = InMemoryKnowledgeStore::new();
        store
            .record_outcome(note(&["slack"], IntentPattern::Execute { plan_id: None }))
            .await;
        let q = RetrievalQuery {
            intent: IntentPattern::Execute { plan_id: None },
            connectors: vec!["slack".into()],
        };
        assert!(store.retrieve_relevant(&q, 0).await.is_empty());
    }

    #[tokio::test]
    async fn replacing_existing_formation_id_keeps_one_entry() {
        let store = InMemoryKnowledgeStore::new();
        let mut n1 = note(&["slack"], IntentPattern::Execute { plan_id: None });
        let mut n2 = n1.clone();
        n2.success_count = 99;
        n2.formation_id = n1.formation_id;
        // Re-record under same FormationId — replaces, doesn't dup.
        n1.success_count = 1;
        store.record_outcome(n1).await;
        store.record_outcome(n2).await;
        assert_eq!(store.len().await, 1);
    }
}
