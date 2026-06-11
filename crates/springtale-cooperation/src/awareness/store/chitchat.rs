//! Scuttlebutt gossip via quickwit's `chitchat` crate.
//!
//! Per COOPERATION.md §8.2: one chitchat node per springtaled process.
//! Agents inside a process publish as sub-keys under the node's
//! `self_node_state`, keyed `a:<uuid>:<field>` (health, role, fuel_pct,
//! last_success, attention_load). Peers observe via `node_states()` and
//! reconstruct `NeighborSnapshot`s.
//!
//! See <https://docs.rs/chitchat/0.10.0/chitchat/>.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chitchat::transport::UdpTransport;
use chitchat::{
    Chitchat, ChitchatConfig, ChitchatHandle, ChitchatId, FailureDetectorConfig, spawn_chitchat,
};
use tokio::sync::Mutex;

use crate::awareness::NeighborSnapshot;
use crate::awareness::bridge::GossipEntry;
use crate::cadence::AgentId;
use crate::error::CooperationError;

use super::trait_::GossipStore;

const FIELD_HEALTH: &str = "health";
const FIELD_ROLE: &str = "role";
const FIELD_FUEL: &str = "fuel_pct";
const FIELD_SUCCESS: &str = "last_success";
const FIELD_ATTENTION: &str = "attention_load";

/// Configuration for a process-level chitchat gossip node.
pub struct ChitchatGossipConfig {
    pub node_id: String,
    pub listen_addr: SocketAddr,
    pub public_addr: SocketAddr,
    pub seeds: Vec<String>,
    pub cluster_id: String,
    pub gossip_interval: Duration,
}

/// Real scuttlebutt gossip. Spawned once per process; every formation
/// shares the same instance through `Arc<dyn GossipStore>`.
pub struct ChitchatGossipStore {
    inner: Arc<Mutex<Chitchat>>,
    _handle: Arc<ChitchatHandle>,
}

impl ChitchatGossipStore {
    pub async fn spawn(cfg: ChitchatGossipConfig) -> Result<Self, CooperationError> {
        let chitchat_id = ChitchatId {
            node_id: cfg.node_id,
            generation_id: 0,
            gossip_advertise_addr: cfg.public_addr,
        };
        let config = ChitchatConfig {
            chitchat_id,
            cluster_id: cfg.cluster_id,
            gossip_interval: cfg.gossip_interval,
            listen_addr: cfg.listen_addr,
            seed_nodes: cfg.seeds,
            failure_detector_config: FailureDetectorConfig::default(),
            marked_for_deletion_grace_period: Duration::from_secs(3600),
            catchup_callback: None,
            extra_liveness_predicate: None,
        };
        let handle = spawn_chitchat(config, Vec::new(), &UdpTransport)
            .await
            .map_err(|e| CooperationError::Gossip(format!("spawn chitchat: {e}")))?;
        let inner = handle.chitchat();
        Ok(Self {
            inner,
            _handle: Arc::new(handle),
        })
    }

    fn agent_key(agent: &AgentId, field: &str) -> String {
        format!("a:{}:{}", agent.0, field)
    }

    fn parse_agent_key(k: &str) -> Option<(String, String)> {
        let mut parts = k.splitn(3, ':');
        if parts.next()? != "a" {
            return None;
        }
        let id = parts.next()?.to_owned();
        let field = parts.next()?.to_owned();
        Some((id, field))
    }
}

#[async_trait]
impl GossipStore for ChitchatGossipStore {
    async fn publish(&self, entry: GossipEntry) {
        let mut cc = self.inner.lock().await;
        let state = cc.self_node_state();
        if let Some(v) = entry.kv.get(FIELD_HEALTH) {
            state.set(Self::agent_key(&entry.agent_id, FIELD_HEALTH), v.clone());
        }
        if let Some(v) = entry.kv.get(FIELD_ROLE) {
            state.set(Self::agent_key(&entry.agent_id, FIELD_ROLE), v.clone());
        }
        if let Some(v) = entry.kv.get(FIELD_FUEL) {
            state.set(Self::agent_key(&entry.agent_id, FIELD_FUEL), v.clone());
        }
        if let Some(v) = entry.kv.get(FIELD_SUCCESS) {
            state.set(Self::agent_key(&entry.agent_id, FIELD_SUCCESS), v.clone());
        }
        if let Some(v) = entry.kv.get(FIELD_ATTENTION) {
            state.set(Self::agent_key(&entry.agent_id, FIELD_ATTENTION), v.clone());
        }
    }

    async fn snapshots(&self) -> Vec<NeighborSnapshot> {
        let cc = self.inner.lock().await;
        // Group every node's key_values by (peer, agent) so each snapshot
        // remembers which process originated it. The peer identifier is
        // the chitchat node's gossip_advertise_addr as a string — same
        // format SWIM's MemberDown event carries, which is what
        // `remove_by_peer` filters against.
        let mut per: HashMap<(String, String), HashMap<String, String>> = HashMap::new();
        let self_peer = {
            let self_id = cc.self_chitchat_id();
            self_id.gossip_advertise_addr.to_string()
        };
        for (id, node_state) in cc.node_states().iter() {
            let peer_id = id.gossip_advertise_addr.to_string();
            for (k, v) in node_state.key_values() {
                if let Some((agent, field)) = Self::parse_agent_key(k) {
                    per.entry((peer_id.clone(), agent))
                        .or_default()
                        .insert(field, v.to_owned());
                }
            }
        }
        per.into_iter()
            .filter_map(|((peer_id, id), kv)| {
                let uuid = uuid::Uuid::parse_str(&id).ok()?;
                let mut entry = GossipEntry {
                    agent_id: AgentId(uuid),
                    kv,
                    peer_id: None,
                    updated_at: Instant::now(),
                };
                // Locally-published entries stay peer_id = None so the
                // sweep doesn't drop them when a peer with the same
                // address label goes down.
                if peer_id != self_peer {
                    entry = entry.with_peer_id(peer_id);
                }
                Some(entry.to_snapshot())
            })
            .collect()
    }

    async fn remove_by_peer(&self, _peer_id: &str) -> usize {
        // Chitchat handles peer expiry natively via marked_for_deletion
        // grace period; forcing a local sweep here would race with the
        // scuttlebutt GC. Return 0 so the caller knows this store is
        // relying on the underlying protocol.
        0
    }

    async fn get(&self, agent_id: &AgentId) -> Option<NeighborSnapshot> {
        self.snapshots()
            .await
            .into_iter()
            .find(|s| s.agent_id == *agent_id)
    }

    async fn remove(&self, agent_id: &AgentId) {
        let mut cc = self.inner.lock().await;
        let state = cc.self_node_state();
        for f in [
            FIELD_HEALTH,
            FIELD_ROLE,
            FIELD_FUEL,
            FIELD_SUCCESS,
            FIELD_ATTENTION,
        ] {
            // chitchat 0.10: `delete` marks the key for GC after its TTL;
            // peers observe the absence on the next gossip round.
            state.delete(&Self::agent_key(agent_id, f));
        }
    }

    async fn len(&self) -> usize {
        self.snapshots().await.len()
    }
}
