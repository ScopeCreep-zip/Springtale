//! Mental-model persistence — rusqlite-backed durable store for
//! `SharedMentalModel` state.
//!
//! Per COOPERATION.md §21: the shared mental model accumulates over time.
//! Without persistence, every process restart resets the formation's
//! accumulated convention / pattern / vocabulary knowledge to empty.
//!
//! File split:
//! - `schema.rs` — SQL migration applied at store open
//! - `rows.rs` — serializable mirrors of the in-memory types (Instant →
//!   Unix epoch seconds; HashMap → rows)
//! - `trait_.rs` — `Store` trait so callers see a pluggable interface
//! - `sqlite.rs` — `SqliteStore` implementation using workspace rusqlite
//! - `error.rs` — narrow `StoreError` with stable COOP-D0NN IDs

pub mod error;
pub mod rows;
pub mod schema;
pub mod sqlite;
pub mod trait_;

pub use error::StoreError;
pub use sqlite::SqliteStore;
pub use trait_::Store;
