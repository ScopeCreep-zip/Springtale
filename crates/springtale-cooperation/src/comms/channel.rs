//! Communication taxonomy — LFCG-aligned classification.
//!
//! Per COOPERATION_IMPLEMENTATION_PLAN.md §19 and Pais et al. CHI 2024:
//! Split Communication-by-Design (intentional agent choices) from
//! Means-of-Communication (infrastructure channels).
//!
//! Communication-by-Design: What agents CHOOSE to communicate.
//! - Callouts (L4D: "WITCH!") — automatic, condition-triggered
//! - Coordination (Siege: "breach on my mark") — intentional, timed
//! - Expression (DRG: "Rock and Stone!") — social, morale
//! - Acknowledgment (Patapon: sing-back) — confirmation of intent
//!
//! Means-of-Communication: HOW the communication is delivered.
//! - Broadcast (all hear) — state broadcasts, cohesion signals
//! - Directed (one target) — protocol messages, directional signals
//! - Environmental (via shared state) — implicit signals, surfaces
//! - Temporal (via ordering) — cadence ticks encode intent

use crate::cadence::{ActionDescriptor, AgentId};

/// Communication-by-Design — what agents choose to communicate.
///
/// Per LFCG taxonomy (Pais et al.): "player communication can be
/// classified by whether it's designed into the system or emerges
/// from player behavior."
#[derive(Debug, Clone)]
pub enum CommunicationIntent {
    /// Automatic condition-triggered alert. L4D: "I'm hurt pretty bad!"
    /// Agent doesn't decide to communicate — the system does.
    Alert {
        source: AgentId,
        condition: String,
        severity: f32,
    },

    /// Deliberate coordination signal. Siege: "breach on my mark."
    /// Agent intentionally communicates to enable a cooperative action.
    Coordinate {
        source: AgentId,
        action: ActionDescriptor,
        timing: Option<u64>, // tick number for synchronized execution
    },

    /// Social/morale expression. DRG: "Rock and Stone!"
    /// No information content — maintains team cohesion.
    Express {
        source: AgentId,
    },

    /// Confirmation of received intent. Patapon: sing-back.
    /// "I received and understand the formation's intent."
    Acknowledge {
        source: AgentId,
        intent_received: String,
    },

    /// Knowledge transfer. Siege: "enemy in kitchen."
    /// Informational, changes recipients' awareness.
    Inform {
        source: AgentId,
        knowledge: String,
        perishable: bool,
    },
}

/// Means-of-Communication — delivery mechanism.
///
/// Per LFCG: orthogonal to intent. The same intent can be delivered
/// via different means depending on formation configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryMeans {
    /// All formation members receive. Broadcast channel.
    /// Good for: alerts, expressions, general coordination.
    Broadcast,

    /// One specific target receives. Point-to-point channel.
    /// Good for: directed coordination, specific information.
    Directed,

    /// Communication via shared environment state changes.
    /// Good for: implicit signals, surface combos.
    Environmental,

    /// Communication via cadence tick timing/intent.
    /// Good for: formation-wide rhythm, intent acknowledgments.
    Temporal,
}

/// Selects the appropriate delivery means for a communication intent.
///
/// Per spec: the means is determined by the intent type, not chosen
/// by the agent. This enforces consistent communication patterns.
pub fn select_delivery(intent: &CommunicationIntent) -> DeliveryMeans {
    match intent {
        CommunicationIntent::Alert { .. } => DeliveryMeans::Broadcast,
        CommunicationIntent::Coordinate { timing: Some(_), .. } => DeliveryMeans::Temporal,
        CommunicationIntent::Coordinate { timing: None, .. } => DeliveryMeans::Directed,
        CommunicationIntent::Express { .. } => DeliveryMeans::Broadcast,
        CommunicationIntent::Acknowledge { .. } => DeliveryMeans::Temporal,
        CommunicationIntent::Inform { perishable: true, .. } => DeliveryMeans::Directed,
        CommunicationIntent::Inform { perishable: false, .. } => DeliveryMeans::Environmental,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_alert_broadcasts() {
        let intent = CommunicationIntent::Alert {
            source: AgentId::new(),
            condition: "low_fuel".into(),
            severity: 0.8,
        };
        assert_eq!(select_delivery(&intent), DeliveryMeans::Broadcast);
    }

    #[test]
    fn test_timed_coordination_temporal() {
        let intent = CommunicationIntent::Coordinate {
            source: AgentId::new(),
            action: ActionDescriptor { kind: "breach".into(), target: None, payload_hash: 0 },
            timing: Some(42),
        };
        assert_eq!(select_delivery(&intent), DeliveryMeans::Temporal);
    }

    #[test]
    fn test_untimed_coordination_directed() {
        let intent = CommunicationIntent::Coordinate {
            source: AgentId::new(),
            action: ActionDescriptor { kind: "assist".into(), target: None, payload_hash: 0 },
            timing: None,
        };
        assert_eq!(select_delivery(&intent), DeliveryMeans::Directed);
    }

    #[test]
    fn test_expression_broadcasts() {
        let intent = CommunicationIntent::Express {
            source: AgentId::new(),
        };
        assert_eq!(select_delivery(&intent), DeliveryMeans::Broadcast);
    }

    #[test]
    fn test_perishable_info_directed() {
        let intent = CommunicationIntent::Inform {
            source: AgentId::new(),
            knowledge: "enemy spotted".into(),
            perishable: true,
        };
        assert_eq!(select_delivery(&intent), DeliveryMeans::Directed);
    }

    #[test]
    fn test_persistent_info_environmental() {
        let intent = CommunicationIntent::Inform {
            source: AgentId::new(),
            knowledge: "route mapping".into(),
            perishable: false,
        };
        assert_eq!(select_delivery(&intent), DeliveryMeans::Environmental);
    }
}
