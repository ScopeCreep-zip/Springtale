//! Communication protocols — multi-layer inter-agent messaging.
//!
//! Per COOPERATION.pdf §19:
//! "Every cooperative game solves inter-agent communication differently.
//! The variation reveals that agents need multiple simultaneous
//! communication layers, not a single channel."
//!
//! Six channel types from different games, each serving a distinct purpose.

use crate::cadence::{ActionDescriptor, AgentId, IntentPattern};
use crate::capability::CapabilityDecl;
use crate::types::ResourceId;

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
    CohesionSignal { source: AgentId },

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
        observed_action: ActionDescriptor,
        inferred_meaning: Option<String>,
    },
}

/// What triggers an automatic state broadcast (L4D callouts).
///
/// From COOPERATION.pdf §19.2:
#[derive(Clone, Debug)]
pub enum BroadcastTrigger {
    /// L4D: "I'm hurt pretty bad"
    HealthBelowThreshold(f32),
    /// L4D: "WITCH!"
    ThreatDetected(String),
    /// L4D: "Pills here!"
    ResourceFound(ResourceId),
    /// L4D: "Man down!"
    AgentDown(AgentId),
    /// "I'm out of ammo"
    CapabilityExhausted(CapabilityDecl),
    /// A member said something the formation can hear (`Speech` or `Burst`
    /// carrier). Thoughts never reach the bus.
    Utterance(crate::utterance::Utterance),
}

/// Target for a protocol message.
///
/// From COOPERATION.pdf §19.2:
#[derive(Clone, Debug)]
pub enum MessageTarget {
    /// Broadcast to all formation members.
    Formation,
    /// Direct message to one agent.
    Specific(AgentId),
    /// Route to whoever can help. MH: nearest capable hunter.
    NearestCapable(CapabilityDecl),
}

/// Content of a state broadcast.
#[derive(Clone, Debug)]
pub struct StateMessage {
    pub content: String,
    pub severity: f32,
}

/// Content of a structured protocol message.
#[derive(Clone, Debug)]
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
        let health = BroadcastTrigger::HealthBelowThreshold(0.3);
        assert!(matches!(health, BroadcastTrigger::HealthBelowThreshold(t) if t == 0.3));

        let threat = BroadcastTrigger::ThreatDetected("rate_limit".into());
        assert!(matches!(threat, BroadcastTrigger::ThreatDetected(ref s) if s == "rate_limit"));

        let resource = BroadcastTrigger::ResourceFound("api_key".into());
        assert!(matches!(resource, BroadcastTrigger::ResourceFound(ref s) if *s == *"api_key"));

        let agent = AgentId::new();
        let down = BroadcastTrigger::AgentDown(agent);
        assert!(matches!(down, BroadcastTrigger::AgentDown(id) if id == agent));

        let exhausted = BroadcastTrigger::CapabilityExhausted("network".into());
        assert!(
            matches!(exhausted, BroadcastTrigger::CapabilityExhausted(ref s) if *s == *"network")
        );
    }

    #[test]
    fn test_comm_channel_variants() {
        let source = AgentId::new();

        let broadcast = CommChannel::StateBroadcast {
            source,
            condition: BroadcastTrigger::HealthBelowThreshold(0.2),
            message: StateMessage {
                content: "low fuel".into(),
                severity: 0.8,
            },
        };
        assert!(
            matches!(broadcast, CommChannel::StateBroadcast { source: s, ref message, .. } if s == source && message.severity == 0.8)
        );

        let cohesion = CommChannel::CohesionSignal { source };
        assert!(matches!(cohesion, CommChannel::CohesionSignal { source: s } if s == source));

        let intent = CommChannel::IntentAcknowledgment {
            source,
            intent_confirmed: IntentPattern::Execute { plan_id: None },
            interpretation: "will process queue".into(),
        };
        assert!(
            matches!(intent, CommChannel::IntentAcknowledgment { source: s, ref interpretation, .. } if s == source && interpretation == "will process queue")
        );
    }
}
