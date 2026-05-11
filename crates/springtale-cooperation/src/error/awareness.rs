use thiserror::Error;

use crate::cadence::AgentId;

#[derive(Debug, Error)]
pub enum AwarenessError {
    #[error("COOP-4001: stale neighbor {agent:?} (age {age} ticks)")]
    StaleNeighbor { agent: AgentId, age: u64 },
    #[error("COOP-4002: gossip bridge disconnected")]
    GossipDisconnected,
}

impl AwarenessError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::StaleNeighbor { .. } => "COOP-4001",
            Self::GossipDisconnected => "COOP-4002",
        }
    }
}
