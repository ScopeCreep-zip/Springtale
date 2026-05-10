//! SWIM liveness via foca 1.0 — cross-process peer-process health.
//!
//! Per COOPERATION.md §8.3: one SWIM node per springtaled process,
//! driven over UDP loopback. Complements `ChitchatGossipStore` (which
//! carries per-agent KV state) by providing explicit Alive / Down /
//! Rejoined lifecycle events that subscribers can react to immediately
//! — much faster than inferring liveness from chitchat's
//! phi-accrual timing.

pub mod events;
pub mod identity;
pub mod node;

pub use events::{SwimEvent, SwimSelfState};
pub use identity::ProcId;
pub use node::{SwimNode, SwimNodeConfig};
