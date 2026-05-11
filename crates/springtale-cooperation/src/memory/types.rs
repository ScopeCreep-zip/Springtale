//! Outcome record + retrieval query types for the global knowledge store.
//!
//! Records are deliberately small — every persisted field maps 1:1 to a
//! cell on the dissolve audit log so a future Qdrant Edge / fastembed
//! backend can build embeddings from this exact JSON shape without a
//! lossy projection.

use serde::{Deserialize, Serialize};

use crate::cadence::IntentPattern;
use crate::momentum::MomentumTier;
use crate::types::FormationId;

/// The raw note deposited by a formation when it dissolves. Stored
/// verbatim by every backend; retrieval transforms a query into
/// `Vec<PriorOutcome>` (which carries a relevance score).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutcomeNote {
    /// Formation that produced this record. Useful for de-dup when a
    /// formation gets re-deployed under the same id.
    pub formation_id: FormationId,
    /// Final intent at dissolve time.
    pub intent: IntentPattern,
    /// Top momentum tier reached during the formation's lifetime — used
    /// as a soft proxy for "did this formation actually get traction or
    /// did it die in Cold."
    pub peak_tier: MomentumTier,
    /// Connectors the formation's members held capabilities on. The
    /// retrieval scorer overlaps this set with the querying intent's
    /// connectors for tag-similarity ranking.
    pub connectors: Vec<String>,
    /// Successes recorded by the formation (best-effort count from the
    /// momentum FSM's `consecutive_successes` at dissolve time).
    pub success_count: u32,
    /// Failures recorded.
    pub failure_count: u32,
    /// Free-form dissolve reason. Stored for audit + future
    /// vector-embedding retrieval.
    pub dissolve_reason: String,
    /// When the deposit was made.
    pub at: chrono::DateTime<chrono::Utc>,
}

/// A query to `retrieve_relevant`. Carries the new formation's intent
/// alongside the connector set it expects to use so the backend can
/// rank prior outcomes by intent similarity and connector overlap.
#[derive(Debug, Clone)]
pub struct RetrievalQuery {
    pub intent: IntentPattern,
    pub connectors: Vec<String>,
}

/// A single retrieved outcome. `score` is the backend's relevance
/// estimate (0.0 .. 1.0; higher = more relevant). The
/// `InMemoryKnowledgeStore` uses tag overlap + intent match; future
/// vector backends will swap in cosine similarity behind the same trait.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorOutcome {
    pub note: OutcomeNote,
    pub score: f32,
}
