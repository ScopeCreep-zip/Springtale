//! Gossip store abstraction — `GossipStore` trait + implementations.
//!
//! The in-memory implementation lives in the `bridge` module alongside
//! `GossipEntry` — `InMemoryGossipStore` implements `GossipStore` directly.
//! The chitchat-backed implementation lives here.

pub mod chitchat;
pub mod trait_;

pub use chitchat::{ChitchatGossipConfig, ChitchatGossipStore};
pub use trait_::GossipStore;
