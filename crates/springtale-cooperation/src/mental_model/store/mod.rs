//! Mental-model persistence — routes through the workspace
//! `springtale-store::StorageBackend` so all cooperation SQL stays in
//! the store crate.
//!
//! Per COOPERATION.md §21: the shared mental model accumulates over time.
//! Without persistence, every process restart resets the formation's
//! accumulated convention / pattern / vocabulary knowledge to empty.
//!
//! File split:
//! - `trait_.rs` — `Store` trait so callers see a pluggable interface
//! - `backend.rs` — `BackendStore` implementation (wraps `Arc<dyn StorageBackend>`)
//! - `rows.rs` — SharedMentalModel ↔ MentalModelBundle conversion
//! - `error.rs` — narrow `StoreError` with stable COOP-D0NN IDs

pub mod backend;
pub mod error;
pub mod rows;
pub mod trait_;

pub use backend::BackendStore;
pub use error::StoreError;
pub use trait_::Store;
