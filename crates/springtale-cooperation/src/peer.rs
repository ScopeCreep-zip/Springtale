//! Peer messages — structural events on the formation broadcast bus.
//!
//! Per COOPERATION_IMPLEMENTATION_PLAN.md §5.6: PeerMsg carries
//! join/leave/down events plus attention redistribution and intent
//! acknowledgments. Broadcast to all members via `broadcast::channel`.

use crate::cadence::AgentId;

/// Structural events broadcast to all formation members.
///
/// These are formation-level events (membership changes, health changes),
/// NOT operational messages (which go through FormationBus channels).
#[derive(Clone, Debug)]
pub enum PeerMsg {
    /// A new agent joined the formation.
    Joined(AgentId),
    /// An agent left the formation voluntarily.
    Left(AgentId),
    /// An agent went down (failed, disconnected).
    AgentDown { id: AgentId, reason: String },
    /// Attention redistribution event (from rally or rebalance).
    AttentionRedistribute { from: AgentId, delta: f32 },
    /// Agent acknowledges the current intent (Patapon sing-back).
    IntentAck { agent: AgentId },
    /// Custom formation-level event.
    Custom(String, serde_json::Value),
}
