//! SQLite-backed knowledge store — outcomes survive process restart.
//!
//! Storage layout: each outcome is serialized as JSON under the
//! key prefix `memory:outcome:{formation_id}` in the existing
//! `config_store` table (managed by `springtale_store::StorageBackend`).
//!
//! Why config-store keys vs a dedicated schema:
//!   - Outcomes are O(formations dissolved per install), not O(events)
//!     — small enough that a linear scan on retrieval is fine for v1.
//!   - The vault encryption layer already covers `config_store`; reusing
//!     it gives encrypted-at-rest for free.
//!   - A dedicated schema can land later by swapping the storage
//!     primitives without changing the trait surface.
//!
//! Retrieval uses the same `score_against` helper as the in-memory
//! backend (intent variant + connector overlap) — the only difference
//! is the source of the note set.

use std::sync::Arc;

use async_trait::async_trait;
use springtale_store::StorageBackend;

use super::store::score_against;
use super::trait_::GlobalKnowledgeStore;
use super::types::{OutcomeNote, PriorOutcome, RetrievalQuery};

const KEY_PREFIX: &str = "memory:outcome:";

pub struct PersistentKnowledgeStore {
    store: Arc<dyn StorageBackend>,
}

impl PersistentKnowledgeStore {
    pub fn new(store: Arc<dyn StorageBackend>) -> Arc<Self> {
        Arc::new(Self { store })
    }

    fn key(formation_id: &crate::types::FormationId) -> String {
        format!("{KEY_PREFIX}{}", formation_id.0)
    }

    async fn load_all(&self) -> Vec<OutcomeNote> {
        let Ok(entries) = self.store.list_config().await else {
            return Vec::new();
        };
        entries
            .into_iter()
            .filter_map(|(k, v)| {
                if !k.starts_with(KEY_PREFIX) {
                    return None;
                }
                serde_json::from_str::<OutcomeNote>(&v).ok()
            })
            .collect()
    }
}

#[async_trait]
impl GlobalKnowledgeStore for PersistentKnowledgeStore {
    async fn record_outcome(&self, note: OutcomeNote) {
        let key = Self::key(&note.formation_id);
        match serde_json::to_string(&note) {
            Ok(json) => {
                if let Err(e) = self.store.set_config(&key, &json).await {
                    tracing::warn!(
                        formation_id = %note.formation_id.0,
                        error = %e,
                        "failed to persist outcome note",
                    );
                }
            }
            Err(e) => tracing::warn!(error = %e, "failed to serialize outcome note"),
        }
    }

    async fn retrieve_relevant(&self, query: &RetrievalQuery, k: usize) -> Vec<PriorOutcome> {
        if k == 0 {
            return Vec::new();
        }
        let mut scored: Vec<PriorOutcome> = self
            .load_all()
            .await
            .into_iter()
            .map(|note| {
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
        let Ok(entries) = self.store.list_config().await else {
            return 0;
        };
        entries
            .into_iter()
            .filter(|(k, _)| k.starts_with(KEY_PREFIX))
            .count()
    }
}
