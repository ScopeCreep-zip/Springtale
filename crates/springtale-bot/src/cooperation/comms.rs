//! Communication protocols — multi-layer inter-agent messaging.
//!
//! Per COOPERATION.pdf §19:
//! "Every cooperative game solves inter-agent communication differently.
//! The variation reveals that agents need multiple simultaneous
//! communication layers, not a single channel."
//!
//! Six channel types from different games, each serving a distinct purpose.

use super::cadence::{AgentId, IntentPattern};

/// Multi-layer communication between formation members.
///
/// Directly from COOPERATION.pdf §19.2:
pub enum CommChannel {
    /// Automatic state broadcasts. L4D survivor callouts.
    /// Triggered by conditions, not by agent decision.
    /// Low cost, high frequency, local scope.
    StateBroadcast {
        source: AgentId,
        condition: BroadcastTrigger,
        message: StateMessage,
    },

    /// Structured protocol messages. MH translated commands.
    /// Designed format, cross-context compatible.
    /// Medium cost, medium frequency.
    ProtocolMessage {
        source: AgentId,
        target: MessageTarget,
        message: ProtocolPayload,
    },

    /// Attention-directing signal. DRG laser pointer, Siege ping.
    /// Points other agents at something specific.
    DirectionalSignal {
        source: AgentId,
        target_object: String,
        urgency: f32,
    },

    /// Social cohesion signal. DRG Rock and Stone.
    /// No information content. Maintains formation morale.
    CohesionSignal {
        source: AgentId,
    },

    /// Intent confirmation. Patapon sing-back.
    /// Agent acknowledges receipt of cadence intent.
    IntentAcknowledgment {
        source: AgentId,
        intent_confirmed: IntentPattern,
        interpretation: String,
    },

    /// Observable behavior as implicit signal. Overcooked chicken-throwing.
    /// Inferred from agent actions, not explicitly sent.
    ImplicitSignal {
        source: AgentId,
        observed_action: String,
        inferred_meaning: Option<String>,
    },
}

/// What triggers an automatic state broadcast (L4D callouts).
///
/// From COOPERATION.pdf §19.2:
pub enum BroadcastTrigger {
    /// L4D: "I'm hurt pretty bad"
    HealthBelowThreshold(f32),
    /// L4D: "WITCH!"
    ThreatDetected(String),
    /// L4D: "Pills here!"
    ResourceFound(String),
    /// L4D: "Man down!"
    AgentDown(AgentId),
    /// "I'm out of ammo"
    CapabilityExhausted(String),
}

/// Target for a protocol message.
///
/// From COOPERATION.pdf §19.2:
pub enum MessageTarget {
    /// Broadcast to all formation members.
    Formation,
    /// Direct message to one agent.
    Specific(AgentId),
    /// Route to whoever can help. MH: nearest capable hunter.
    NearestCapable(String),
}

/// Content of a state broadcast.
pub struct StateMessage {
    pub content: String,
    pub severity: f32,
}

/// Content of a structured protocol message.
pub struct ProtocolPayload {
    pub message_type: String,
    pub data: serde_json::Value,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_broadcast_trigger_variants() {
        let _health = BroadcastTrigger::HealthBelowThreshold(0.3);
        let _threat = BroadcastTrigger::ThreatDetected("rate_limit".into());
        let _resource = BroadcastTrigger::ResourceFound("api_key".into());
        let _down = BroadcastTrigger::AgentDown(AgentId::new());
        let _exhausted = BroadcastTrigger::CapabilityExhausted("network".into());
    }

    #[test]
    fn test_comm_channel_variants() {
        let source = AgentId::new();

        let _broadcast = CommChannel::StateBroadcast {
            source,
            condition: BroadcastTrigger::HealthBelowThreshold(0.2),
            message: StateMessage { content: "low fuel".into(), severity: 0.8 },
        };

        let _cohesion = CommChannel::CohesionSignal { source };

        let _intent = CommChannel::IntentAcknowledgment {
            source,
            intent_confirmed: IntentPattern::Execute { plan_id: None },
            interpretation: "will process queue".into(),
        };
    }
}
