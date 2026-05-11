//! `FormationGossipBus` — the seam every cross-formation transport
//! implements. Two implementations land with G6:
//!
//! - [`InMemoryFormationGossipBus`](super::bus::InMemoryFormationGossipBus)
//!   — single-process, `tokio::sync::broadcast`-backed. The default for
//!   tests and for single-springtaled deployments.
//! - A chitchat-backed implementation (added in `apps/springtaled` next
//!   to the per-agent `ChitchatGossipStore`) that publishes formation
//!   keys under the same chitchat node.
//!
//! Per Quickwit's chitchat algorithm spec, only the *owning* node
//! writes to its own keys. A formation gossiping its `FormationView`
//! never modifies a peer's entry — the bus just exposes the read side
//! of every other node's writes.

use async_trait::async_trait;
use tokio::sync::broadcast;

use super::types::{FormationDelta, FormationOutcome, FormationView};

#[async_trait]
pub trait FormationGossipBus: Send + Sync {
    /// Publish (or replace) the caller's own running-state snapshot.
    /// Idempotent — re-publishing the same view is a no-op for peers.
    async fn publish_view(&self, view: FormationView);

    /// Publish a terminal-outcome record. Sticky: subscribers that come
    /// online later still see it, until the outcome retention window
    /// elapses (implementation-defined; the chitchat impl uses
    /// scuttlebutt expiry).
    async fn publish_outcome(&self, outcome: FormationOutcome);

    /// Snapshot every currently-known peer formation view. Useful for
    /// Fever-tier orchestration where the AI orchestrator wants the
    /// current cross-formation state in one shot rather than streaming.
    async fn snapshot(&self) -> Vec<FormationView>;

    /// Subscribe to the merged delta stream — both view updates and
    /// terminal outcomes arrive here. Callers see only deltas
    /// originating *outside* their own formation; the bus filters by
    /// `excluding` (the subscriber's own `FormationId`) so a formation
    /// never receives its own broadcasts.
    fn subscribe(
        &self,
        excluding: crate::types::FormationId,
    ) -> broadcast::Receiver<FormationDelta>;
}
