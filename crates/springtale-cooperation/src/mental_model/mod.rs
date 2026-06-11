//! Shared mental model — accumulated context that enables anticipatory cooperation.
//!
//! Per COOPERATION.pdf §21: "Teams that share a mental model cooperate
//! with less communication overhead. The model must be built, not assumed."

pub mod external_workspaces;
pub mod graph;
pub mod learning;
pub mod store;
pub mod types;

pub use external_workspaces::{
    DiscoveredWorkspace, ExternalWorkspaceDirectory, ExternalWorkspaceEntry, WorkspaceProvenance,
    merge_gossip_delta,
};
pub use store::{BackendStore, Store, StoreError};
pub use types::{Convention, CooperationPattern, DomainEntry, SharedMentalModel, VocabularyEntry};
