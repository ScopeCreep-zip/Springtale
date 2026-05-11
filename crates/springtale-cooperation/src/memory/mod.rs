//! G2 — global cross-formation knowledge store (`COOPERATION.md §21`).
//!
//! Per-formation `SharedMentalModel` already persists via
//! [`super::mental_model::store::Store`]; that's intra-formation memory
//! (conventions, vocabulary, patterns learned during a single mission).
//! This module adds the *cross-formation* layer the spec calls for:
//! every formation that finishes deposits a `FormationOutcome` here, and
//! new formations seed their initial mental model from prior outcomes
//! relevant to their intent.
//!
//! ## Design (plan I2 / `COOPERATION_IMPLEMENTATION_PLAN.md §12.6`)
//!
//! Two-layer note-graph (A-MEM pattern):
//!
//! 1. **Outcome notes** — each note is `{intent, momentum_reached, success_count,
//!    failure_count, dissolve_reason, connectors_involved, deposited_at}`.
//!    A new formation queries by *intent similarity* to its own intent
//!    and gets back the top-K most relevant prior outcomes.
//!
//! 2. **Relational links** — outcomes referring to the same intent
//!    pattern or sharing connector capabilities form an implicit graph
//!    that retrieve_relevant traverses (currently via tag-overlap; a
//!    later impl can swap to vector similarity behind the same trait).
//!
//! ## Trait seam vs concrete backend
//!
//! - [`GlobalKnowledgeStore`] is the seam every backend implements.
//! - [`InMemoryKnowledgeStore`] — `DashMap`-backed, zero-disk. The
//!   default for tests + single-process deployments + CLI dry-runs.
//! - [`PersistentKnowledgeStore`] — backed by the existing
//!   `springtale_store::StorageBackend` (SQLite under the vault dir).
//!   Production default; outcomes survive process restart.
//!
//! A future Qdrant Edge + fastembed-rs backend can land behind the same
//! trait without changing any call site — the contract is
//! `record_outcome` / `retrieve_relevant`, not "vector search."
//!
//! ## When call sites invoke it
//!
//! - **Dissolve** (`bot::runtime::tick_steps::handle_command::Dissolve`)
//!   calls `record_outcome` so the deposit survives the formation.
//!   This is the same site the G6 cross-formation gossip bus emits
//!   `FormationOutcome` from — both paths use the same outcome record.
//! - **Spawn** (`bot::cooperation::lifecycle::spawn_formation`) calls
//!   `retrieve_relevant(intent, k=5)` and seeds the new formation's
//!   `SharedMentalModel.history` with the returned outcomes so the
//!   first orchestration tick has context.
//!
//! ## Security
//!
//! Trust zone: **Z4 (cooperation-internal)** for retrieval; **Z2
//! (daemon)** for persistence (the SQLite `config_store` is the
//! daemon's own data, vault-encrypted at rest). Threats covered in
//! [`docs/intended-arch/COOPERATION_SECURITY_REVIEW.md §memory`](../../../docs/intended-arch/COOPERATION_SECURITY_REVIEW.md):
//! malicious connectors influencing future formations through
//! poisoned outcomes (1), information leak via persistent records
//! (5), unbounded corpus growth (4). Retrieval is bounded to top-K
//! (K≤5 default) so a flood of poison notes still surfaces ≤5 to
//! any new formation. Scorer is deterministic + side-effect-free —
//! re-publishing a note doesn't raise its score. `wipe` ops are the
//! recovery primitive (per-formation or full).

pub mod persistent;
pub mod store;
pub mod trait_;
pub mod types;

pub use persistent::PersistentKnowledgeStore;
pub use store::InMemoryKnowledgeStore;
pub use trait_::GlobalKnowledgeStore;
pub use types::{OutcomeNote, PriorOutcome, RetrievalQuery};
