//! Awareness system — local neighbor perception.
//!
//! Per COOPERATION.pdf §8: "Total War morale as composite local signal."
//! Agents perceive neighbors through local awareness — partial information,
//! local decisions. Available at Warming+ tier (§7 capability table).

pub mod bridge;
pub mod store;
pub mod swim;
mod types;

pub use bridge::{GossipEntry, InMemoryGossipStore};
pub use store::{ChitchatGossipConfig, ChitchatGossipStore, GossipStore};
pub use swim::{ProcId, SwimEvent, SwimNode, SwimNodeConfig, SwimSelfState};
pub use types::{LocalAwareness, NeighborSnapshot, RoleSignature};
