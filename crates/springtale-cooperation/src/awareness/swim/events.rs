//! SWIM lifecycle events — subscribers see process-level liveness
//! transitions without needing to poll foca's `Member` state.

use std::net::SocketAddr;

/// Observable peer-process liveness transitions.
#[derive(Debug, Clone)]
pub enum SwimEvent {
    /// Foca promoted a peer to Alive (joined, rejoined, or first seen).
    MemberUp(SocketAddr),
    /// Foca confirmed a peer is Down — subscribers should prune state
    /// attributed to this process (gossip entries, flex-chain workers).
    MemberDown(SocketAddr),
    /// A peer bumped its incarnation (restart rejoin).
    MemberRejoined(SocketAddr),
    /// Foca's own state transitioned (Active / Idle / Defunct).
    /// Idle means this node has no peers to probe; Defunct means it
    /// was declared Down and hasn't rejoined.
    SelfState(SwimSelfState),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwimSelfState {
    Active,
    Idle,
    Defunct,
}
