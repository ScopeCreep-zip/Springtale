//! Gossip bridge — maps chitchat KV and foca liveness to NeighborSnapshot.
//!
//! Per COOPERATION.md §8: gossip protocol for awareness exchange.
//! `chitchat` provides scuttlebutt-style KV propagation (each node publishes
//! key-value pairs, peers gossip them to convergence).
//! `foca` provides SWIM-based liveness detection (ping/ping-req/suspect).
//!
//! In single-process mode: all members share the same Rust process, so
//! gossip is an in-memory KV store keyed by AgentId. No network needed.
//! Cross-process gossip uses `ChitchatGossipStore` — activated via
//! `CooperationConfig::cross_process` in the runtime config. The Veilid
//! DHT transport (the only remaining Phase 3 deferral) will swap under
//! the same trait when it lands.

use std::collections::HashMap;
use std::time::Instant;

use async_trait::async_trait;
use dashmap::DashMap;

use crate::cadence::AgentId;
use crate::supervision::Liveness;
use crate::types::AgentHealth;

use super::store::GossipStore;
use super::types::{NeighborSnapshot, RoleSignature};

/// Keys used in the gossip KV store. Per chitchat: each node publishes
/// its own state under string keys, peers read all keys.
const KEY_HEALTH: &str = "health";
const KEY_ROLE: &str = "role";
const KEY_FUEL_PCT: &str = "fuel_pct";
const KEY_LAST_SUCCESS: &str = "last_success";
const KEY_ATTENTION: &str = "attention_load";

/// A gossip entry published by one agent.
///
/// `peer_id` identifies the process that originated this entry:
/// - `None` for locally-published entries (same process as the reader).
/// - `Some(addr)` for entries learned from a remote peer via chitchat /
///   cross-process gossip. The string is the peer's stable identifier
///   (usually a SocketAddr). SWIM's `MemberDown` event carries the same
///   identifier, so `GossipStore::remove_by_peer` can sweep every entry
///   the dead peer published without needing a separate peer→agent map.
#[derive(Debug, Clone)]
pub struct GossipEntry {
    pub agent_id: AgentId,
    pub kv: HashMap<String, String>,
    pub peer_id: Option<String>,
    pub updated_at: Instant,
}

impl GossipEntry {
    /// Local-publish constructor. Leaves `peer_id = None` because the
    /// entry is owned by this process. Remote entries arrive via
    /// `ChitchatGossipStore` which stamps `peer_id` with the sender.
    pub fn from_state(
        agent_id: AgentId,
        health: &AgentHealth,
        role_name: &str,
        fuel_pct: f32,
        last_success: bool,
        attention_load: f32,
    ) -> Self {
        let mut kv = HashMap::new();
        // Health serializes as JSON so `Degraded{recovery_count:2}` and
        // `Dead{recoverable:true}` round-trip without data loss — the
        // old Debug-string format collapsed both to their variant name
        // and the parser had to guess the fields.
        let health_json =
            serde_json::to_string(health).unwrap_or_else(|_| "\"Operational\"".to_owned());
        kv.insert(KEY_HEALTH.to_owned(), health_json);
        kv.insert(KEY_ROLE.to_owned(), role_name.to_owned());
        kv.insert(KEY_FUEL_PCT.to_owned(), format!("{fuel_pct:.2}"));
        kv.insert(KEY_LAST_SUCCESS.to_owned(), last_success.to_string());
        kv.insert(KEY_ATTENTION.to_owned(), format!("{attention_load:.2}"));
        Self {
            agent_id,
            kv,
            peer_id: None,
            updated_at: Instant::now(),
        }
    }

    /// Stamp a peer identifier onto this entry — called by cross-process
    /// gossip adapters when decoding an entry received from a remote peer.
    pub fn with_peer_id(mut self, peer_id: impl Into<String>) -> Self {
        self.peer_id = Some(peer_id.into());
        self
    }

    /// Convert gossip entry back to a NeighborSnapshot.
    pub fn to_snapshot(&self) -> NeighborSnapshot {
        let role = self
            .kv
            .get(KEY_ROLE)
            .map(|s| RoleSignature::parse(s))
            .unwrap_or_else(|| {
                tracing::warn!(
                    agent = %self.agent_id,
                    "gossip: missing role key, defaulting to Custom(empty)"
                );
                RoleSignature::Custom(String::new())
            });
        let fuel_remaining_pct = self
            .kv
            .get(KEY_FUEL_PCT)
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| {
                tracing::warn!(agent = %self.agent_id, "gossip: missing/invalid fuel_pct, defaulting to 1.0");
                1.0
            });
        let last_action_success = self
            .kv
            .get(KEY_LAST_SUCCESS)
            .map(|v| v == "true")
            .unwrap_or_else(|| {
                tracing::warn!(agent = %self.agent_id, "gossip: missing last_success, defaulting to false (safe assumption)");
                false
            });
        let attention_load = self
            .kv
            .get(KEY_ATTENTION)
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| {
                tracing::warn!(agent = %self.agent_id, "gossip: missing/invalid attention_load, defaulting to 0.0");
                0.0
            });
        NeighborSnapshot {
            agent_id: self.agent_id,
            health: self.parse_health(),
            role,
            fuel_remaining_pct,
            last_action_success,
            attention_load,
            liveness: Liveness::Alive,
            last_updated: self.updated_at,
        }
    }

    /// Parse the serialized health field. JSON first (the current
    /// format), then fall back to the legacy Debug-string format for
    /// entries that were published before the serialization change.
    fn parse_health(&self) -> AgentHealth {
        let Some(raw) = self.kv.get(KEY_HEALTH) else {
            return AgentHealth::Operational;
        };
        if let Ok(h) = serde_json::from_str::<AgentHealth>(raw) {
            return h;
        }
        // Legacy Debug-format fallback. Still better than returning a
        // default when downstream consumers care about the variant; the
        // inner fields are genuinely lost in that format so we leave
        // them at conservative defaults and log once.
        tracing::warn!(
            agent = %self.agent_id,
            raw = %raw,
            "gossip: health field in legacy Debug format; upgrading to JSON on next publish"
        );
        match raw.as_str() {
            "Operational" => AgentHealth::Operational,
            "Incapacitated" => AgentHealth::Incapacitated,
            s if s.starts_with("Degraded") => AgentHealth::Degraded { recovery_count: 1 },
            s if s.starts_with("Dead") => AgentHealth::Dead { recoverable: false },
            _ => AgentHealth::Operational,
        }
    }
}

/// In-memory gossip store — single-process implementation of [`GossipStore`].
///
/// Per chitchat: each node publishes KV pairs, peers read all keys.
/// This in-memory version stores all entries in a `DashMap` for lock-free
/// reads and interior-mutable writes (so the trait methods can stay
/// `&self`, which is what `Arc<dyn GossipStore>` callers need).
///
/// Cross-process deployments swap to
/// [`ChitchatGossipStore`](super::store::ChitchatGossipStore) under the
/// same trait.
#[derive(Debug, Default)]
pub struct InMemoryGossipStore {
    entries: DashMap<AgentId, GossipEntry>,
}

impl InMemoryGossipStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the store has any entries. Exposed for tests and
    /// observability (the trait keeps the API surface focused).
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[async_trait]
impl GossipStore for InMemoryGossipStore {
    async fn publish(&self, entry: GossipEntry) {
        self.entries.insert(entry.agent_id, entry);
    }

    async fn snapshots(&self) -> Vec<NeighborSnapshot> {
        self.entries
            .iter()
            .map(|r| r.value().to_snapshot())
            .collect()
    }

    async fn remove_by_peer(&self, peer_id: &str) -> usize {
        // Collect matching ids first (DashMap rejects concurrent remove
        // inside iter); then do the removals.
        let to_remove: Vec<AgentId> = self
            .entries
            .iter()
            .filter(|e| e.value().peer_id.as_deref() == Some(peer_id))
            .map(|e| *e.key())
            .collect();
        let count = to_remove.len();
        for id in to_remove {
            self.entries.remove(&id);
        }
        count
    }

    async fn get(&self, agent_id: &AgentId) -> Option<NeighborSnapshot> {
        self.entries.get(agent_id).map(|r| r.value().to_snapshot())
    }

    async fn remove(&self, agent_id: &AgentId) {
        self.entries.remove(agent_id);
    }

    async fn len(&self) -> usize {
        self.entries.len()
    }
}

/// Map foca member state to our Liveness enum.
///
/// Per foca: `State::Alive`, `State::Suspect`, `State::Down`.
/// Per COOPERATION.md §8: align with K8s liveness probes.
pub fn foca_state_to_liveness(alive: bool, suspect: bool) -> Liveness {
    if suspect {
        Liveness::Suspect { missed_ticks: 1 }
    } else if alive {
        Liveness::Alive
    } else {
        Liveness::Down { since_tick: crate::tick::TickId::ZERO }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn publish_and_read_snapshot() {
        let store = InMemoryGossipStore::new();
        let agent = AgentId::new();

        store
            .publish(GossipEntry::from_state(
                agent,
                &AgentHealth::Operational,
                "General",
                0.85,
                true,
                0.3,
            ))
            .await;

        let snap = store.get(&agent).await.unwrap();
        assert_eq!(snap.agent_id, agent);
        assert_eq!(snap.role, RoleSignature::General);
        assert!((snap.fuel_remaining_pct - 0.85).abs() < 0.01);
        assert!(snap.last_action_success);
        assert!((snap.attention_load - 0.3).abs() < 0.01);
    }

    #[tokio::test]
    async fn multiple_agents_snapshots() {
        let store = InMemoryGossipStore::new();
        let a = AgentId::new();
        let b = AgentId::new();

        store
            .publish(GossipEntry::from_state(
                a,
                &AgentHealth::Operational,
                "General",
                1.0,
                true,
                0.5,
            ))
            .await;
        store
            .publish(GossipEntry::from_state(
                b,
                &AgentHealth::Degraded { recovery_count: 1 },
                "Support",
                0.3,
                false,
                0.1,
            ))
            .await;

        assert_eq!(store.len().await, 2);
        let snaps = store.snapshots().await;
        assert_eq!(snaps.len(), 2);
    }

    #[tokio::test]
    async fn remove_agent() {
        let store = InMemoryGossipStore::new();
        let agent = AgentId::new();
        store
            .publish(GossipEntry::from_state(
                agent,
                &AgentHealth::Operational,
                "General",
                1.0,
                true,
                0.0,
            ))
            .await;
        assert_eq!(store.len().await, 1);
        store.remove(&agent).await;
        assert!(store.is_empty());
    }

    #[test]
    fn foca_mapping() {
        assert!(matches!(
            foca_state_to_liveness(true, false),
            Liveness::Alive
        ));
        assert!(matches!(
            foca_state_to_liveness(false, true),
            Liveness::Suspect { .. }
        ));
        assert!(matches!(
            foca_state_to_liveness(false, false),
            Liveness::Down { .. }
        ));
    }

    #[test]
    fn health_roundtrip() {
        let agent = AgentId::new();
        let entry = GossipEntry::from_state(
            agent,
            &AgentHealth::Incapacitated,
            "Information",
            0.0,
            false,
            0.0,
        );
        let snap = entry.to_snapshot();
        assert!(matches!(snap.health, AgentHealth::Incapacitated));
        assert_eq!(snap.role, RoleSignature::Information);
    }
}
