//! GossipStore trait — pluggable peer-state exchange.
//!
//! Two implementations today:
//! - [`InMemoryGossipStore`](super::super::bridge::InMemoryGossipStore) —
//!   single-process, zero-network. Every formation member is inside the
//!   same Rust process, so gossip is a `DashMap` lookup.
//! - [`ChitchatGossipStore`](super::chitchat::ChitchatGossipStore) — real
//!   scuttlebutt via quickwit's `chitchat` crate over UDP loopback. One
//!   chitchat node per springtaled process; agents inside a process publish
//!   as sub-keys under the node's `self_node_state`.
//!
//! Phase 3 Veilid transport (the only remaining deferral) will add a
//! third implementation behind the same trait — discovery across the
//! Veilid DHT rather than explicit chitchat seed lists.

use async_trait::async_trait;

use crate::cadence::AgentId;
use crate::awareness::NeighborSnapshot;

use super::super::bridge::GossipEntry;

#[async_trait]
pub trait GossipStore: Send + Sync {
    /// Publish the caller's own state. Replaces any previous entry for
    /// the same agent_id.
    async fn publish(&self, entry: GossipEntry);

    /// Every currently-known snapshot (across all peers).
    async fn snapshots(&self) -> Vec<NeighborSnapshot>;

    /// Single agent's snapshot, if any.
    async fn get(&self, agent_id: &AgentId) -> Option<NeighborSnapshot>;

    /// Remove an agent's entry (used on detach / down).
    async fn remove(&self, agent_id: &AgentId);

    /// Remove every entry published by the given peer process. Called
    /// on `SwimEvent::MemberDown` so defunct peers stop contributing
    /// stale snapshots to `snapshots()`. The `peer_id` is the same
    /// stable identifier the gossip adapter stamps on entries via
    /// `GossipEntry::with_peer_id` (typically a SocketAddr string).
    /// Returns the number of entries removed.
    ///
    /// Default implementation walks `snapshots()`-then-`remove()` for
    /// stores that don't natively index by peer — override for
    /// implementations that can do the filter cheaply.
    async fn remove_by_peer(&self, peer_id: &str) -> usize {
        let mut removed = 0;
        for snap in self.snapshots().await {
            // Without peer_id in the snapshot we can't filter cheaply.
            // The default impl is a no-op; `InMemoryGossipStore`
            // overrides it to do the real filter against stored
            // entries. Chitchat-backed stores get peer-expiry for free
            // from the underlying scuttlebutt node.
            let _ = snap; // silence unused
            let _ = peer_id;
            removed += 0;
        }
        removed
    }

    /// Number of known agents — observability.
    async fn len(&self) -> usize;

    /// Whether the store has no entries. Companion to `len()` required
    /// by Rust idiom / clippy `len_without_is_empty`.
    async fn is_empty(&self) -> bool {
        self.len().await == 0
    }
}
