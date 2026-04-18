//! L5 global re-plan — Consensus-Based Bundle Algorithm (CBBA).
//!
//! Triggered on cascade detection or intent change. Each agent greedily
//! builds an ordered task bundle (scored via `utility/`), then gossips bids
//! with neighbors over FormationBus until the assignment is conflict-free.
//! Reference: Choi, Brunet & How, "Consensus-based decentralized auctions
//! for robust task allocation" (MIT ACL).

pub mod cbba;
pub mod trigger;
