//! Attention economy — zero-sum workload distribution.
//!
//! Per COOPERATION.md §9: Army of Two aggro model. Whoever does the
//! most work draws attention; others get freedom to act independently.
//!
//! `AttentionEconomy` — the data model (clone-friendly, single-threaded).
//! `AttentionBroker` — ArcSwap wrapper for concurrent read-heavy access.

pub mod broker;
pub mod economy;

pub use broker::AttentionBroker;
pub use economy::AttentionEconomy;
