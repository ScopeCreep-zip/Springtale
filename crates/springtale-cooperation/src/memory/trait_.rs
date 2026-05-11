//! `GlobalKnowledgeStore` — the seam every cross-formation memory
//! backend implements.
//!
//! Two implementations live in this crate:
//!   - `InMemoryKnowledgeStore` (default, zero-disk, fast)
//!   - `PersistentKnowledgeStore` (SQLite-backed via the existing
//!     `springtale_store::StorageBackend`)
//!
//! A future Qdrant Edge + fastembed-rs implementation can land behind
//! the same trait — call sites in `lifecycle::spawn_formation` and
//! `tick_steps::handle_command::Dissolve` only depend on the trait.

use async_trait::async_trait;

use super::types::{OutcomeNote, PriorOutcome, RetrievalQuery};

#[async_trait]
pub trait GlobalKnowledgeStore: Send + Sync {
    /// Persist an outcome record. Called once per formation dissolve.
    /// Failures are surfaced via `tracing` at the call site — the
    /// dissolve path must not abort if persistence fails.
    async fn record_outcome(&self, note: OutcomeNote);

    /// Return up to `k` prior outcomes ranked by relevance to `query`.
    /// Empty result is normal (cold-start; no formations have dissolved
    /// yet). Implementations should never block longer than ~10ms here —
    /// it runs on the spawn-formation hot path.
    async fn retrieve_relevant(&self, query: &RetrievalQuery, k: usize) -> Vec<PriorOutcome>;

    /// Total stored outcome count. Observability hook only — not
    /// performance-critical, but must be cheap (`O(1)` lookup).
    async fn len(&self) -> usize;

    /// True when no outcomes have been recorded. Standard companion to
    /// `len()` per clippy `len_without_is_empty`.
    async fn is_empty(&self) -> bool {
        self.len().await == 0
    }
}
