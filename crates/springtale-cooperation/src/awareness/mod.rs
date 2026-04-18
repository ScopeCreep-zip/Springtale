//! Awareness system — local neighbor perception.
//!
//! Per COOPERATION.pdf §8: "Total War morale as composite local signal."
//! Agents perceive neighbors through local awareness — partial information,
//! local decisions. Available at Warming+ tier (§7 capability table).

pub mod bridge;
pub mod gossip;
mod types;

pub use bridge::{GossipEntry, InMemoryGossipStore};
pub use types::{LocalAwareness, NeighborSnapshot};
