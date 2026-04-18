//! Gossip bridge — maps chitchat KV and foca liveness to NeighborSnapshot.
//!
//! Per COOPERATION.md §8: gossip protocol for awareness exchange.
//! `chitchat` provides scuttlebutt-style KV propagation (each node publishes
//! key-value pairs, peers gossip them to convergence).
//! `foca` provides SWIM-based liveness detection (ping/ping-req/suspect).
//!
//! In single-process mode: all members share the same Rust process, so
//! gossip is an in-memory KV store keyed by AgentId. No network needed.
//! Cross-process gossip (Phase 3/Veilid) swaps the transport under the
//! same trait.

use std::collections::HashMap;
use std::time::Instant;

use crate::cadence::AgentId;
use crate::supervision::Liveness;
use crate::types::AgentHealth;

use super::types::NeighborSnapshot;

/// Keys used in the gossip KV store. Per chitchat: each node publishes
/// its own state under string keys, peers read all keys.
const KEY_HEALTH: &str = "health";
const KEY_ROLE: &str = "role";
const KEY_FUEL_PCT: &str = "fuel_pct";
const KEY_LAST_SUCCESS: &str = "last_success";
const KEY_ATTENTION: &str = "attention_load";

/// A gossip entry published by one agent.
#[derive(Debug, Clone)]
pub struct GossipEntry {
    pub agent_id: AgentId,
    pub kv: HashMap<String, String>,
    pub updated_at: Instant,
}

impl GossipEntry {
    pub fn from_state(
        agent_id: AgentId,
        health: &AgentHealth,
        role_name: &str,
        fuel_pct: f32,
        last_success: bool,
        attention_load: f32,
    ) -> Self {
        let mut kv = HashMap::new();
        kv.insert(KEY_HEALTH.to_owned(), format!("{health:?}"));
        kv.insert(KEY_ROLE.to_owned(), role_name.to_owned());
        kv.insert(KEY_FUEL_PCT.to_owned(), format!("{fuel_pct:.2}"));
        kv.insert(KEY_LAST_SUCCESS.to_owned(), last_success.to_string());
        kv.insert(KEY_ATTENTION.to_owned(), format!("{attention_load:.2}"));
        Self {
            agent_id,
            kv,
            updated_at: Instant::now(),
        }
    }

    /// Convert gossip entry back to a NeighborSnapshot.
    pub fn to_snapshot(&self) -> NeighborSnapshot {
        let role_name = self.kv.get(KEY_ROLE).cloned().unwrap_or_else(|| {
            tracing::warn!(agent = %self.agent_id, "gossip: missing role key, defaulting to empty");
            String::new()
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
            role_name,
            fuel_remaining_pct,
            last_action_success,
            attention_load,
            liveness: Liveness::Alive,
            last_updated: self.updated_at,
        }
    }

    fn parse_health(&self) -> AgentHealth {
        match self.kv.get(KEY_HEALTH).map(|s| s.as_str()) {
            Some("Operational") => AgentHealth::Operational,
            Some("Incapacitated") => AgentHealth::Incapacitated,
            Some(s) if s.starts_with("Degraded") => AgentHealth::Degraded { recovery_count: 1 },
            Some(s) if s.starts_with("Dead") => AgentHealth::Dead { recoverable: false },
            _ => AgentHealth::Operational,
        }
    }
}

/// In-memory gossip store — single-process implementation.
///
/// Per chitchat: each node publishes KV pairs, peers read all keys.
/// This in-memory version stores all entries in a HashMap.
/// Cross-process (Phase 3) will swap to chitchat's network transport.
#[derive(Debug, Default)]
pub struct InMemoryGossipStore {
    entries: HashMap<AgentId, GossipEntry>,
}

impl InMemoryGossipStore {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Publish this agent's state (chitchat: set_kv).
    pub fn publish(&mut self, entry: GossipEntry) {
        self.entries.insert(entry.agent_id, entry);
    }

    /// Read all published snapshots (chitchat: iterate_kv_pairs).
    pub fn snapshots(&self) -> Vec<NeighborSnapshot> {
        self.entries.values().map(|e| e.to_snapshot()).collect()
    }

    /// Get a specific agent's snapshot.
    pub fn get(&self, agent_id: &AgentId) -> Option<NeighborSnapshot> {
        self.entries.get(agent_id).map(|e| e.to_snapshot())
    }

    /// Remove an agent (foca: member left/down).
    pub fn remove(&mut self, agent_id: &AgentId) {
        self.entries.remove(agent_id);
    }

    /// Number of known agents.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
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
        Liveness::Down { since_tick: 0 }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn publish_and_read_snapshot() {
        let mut store = InMemoryGossipStore::new();
        let agent = AgentId::new();

        store.publish(GossipEntry::from_state(
            agent,
            &AgentHealth::Operational,
            "General",
            0.85,
            true,
            0.3,
        ));

        let snap = store.get(&agent).unwrap();
        assert_eq!(snap.agent_id, agent);
        assert_eq!(snap.role_name, "General");
        assert!((snap.fuel_remaining_pct - 0.85).abs() < 0.01);
        assert!(snap.last_action_success);
        assert!((snap.attention_load - 0.3).abs() < 0.01);
    }

    #[test]
    fn multiple_agents_snapshots() {
        let mut store = InMemoryGossipStore::new();
        let a = AgentId::new();
        let b = AgentId::new();

        store.publish(GossipEntry::from_state(
            a, &AgentHealth::Operational, "General", 1.0, true, 0.5,
        ));
        store.publish(GossipEntry::from_state(
            b, &AgentHealth::Degraded { recovery_count: 1 }, "Support", 0.3, false, 0.1,
        ));

        assert_eq!(store.len(), 2);
        let snaps = store.snapshots();
        assert_eq!(snaps.len(), 2);
    }

    #[test]
    fn remove_agent() {
        let mut store = InMemoryGossipStore::new();
        let agent = AgentId::new();
        store.publish(GossipEntry::from_state(
            agent, &AgentHealth::Operational, "General", 1.0, true, 0.0,
        ));
        assert_eq!(store.len(), 1);
        store.remove(&agent);
        assert!(store.is_empty());
    }

    #[test]
    fn foca_mapping() {
        assert!(matches!(foca_state_to_liveness(true, false), Liveness::Alive));
        assert!(matches!(foca_state_to_liveness(false, true), Liveness::Suspect { .. }));
        assert!(matches!(foca_state_to_liveness(false, false), Liveness::Down { .. }));
    }

    #[test]
    fn health_roundtrip() {
        let agent = AgentId::new();
        let entry = GossipEntry::from_state(
            agent, &AgentHealth::Incapacitated, "Information", 0.0, false, 0.0,
        );
        let snap = entry.to_snapshot();
        assert!(matches!(snap.health, AgentHealth::Incapacitated));
        assert_eq!(snap.role_name, "Information");
    }
}
