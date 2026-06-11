//! Runtime-side persisted token quota (Phase-7 audit Finding D).
//!
//! Implements [`springtale_ai::TokenQuota`] over the daemon's
//! [`springtale_store::StorageBackend`] so per-bot daily counters
//! survive daemon restart. The trait lives in `springtale-ai` and the
//! storage lives in `springtale-store`; this crate is where they
//! meet, per the crate-structure rule that ai must not depend on
//! store directly.

pub mod sqlite;

pub use sqlite::SqliteTokenQuota;
