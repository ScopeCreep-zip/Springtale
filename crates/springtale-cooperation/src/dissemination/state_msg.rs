use serde::{Deserialize, Serialize};

use crate::cadence::AgentId;
use crate::momentum::MomentumTier;
use crate::types::AgentHealth;

/// Liveness + status broadcasts across a formation.
///
/// Kept intentionally narrow: anything that changes *task* flow goes through
/// routing; anything that changes *state* visible to peers goes here. The
/// split lives here rather than being inlined into a god-enum on the bus so
/// adding a new signal is a one-line change in this file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateMessage {
    AgentHealthChanged {
        agent: AgentId,
        health: AgentHealth,
    },
    MomentumChanged {
        tier: MomentumTier,
    },
    RallyConsumed {
        by: AgentId,
        remaining: u32,
    },
    RallyReplenished {
        remaining: u32,
    },
    IntentAck {
        agent: AgentId,
        intent_sequence: u64,
    },
    AgentLeft {
        agent: AgentId,
    },
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn all_variants_constructible() {
        let a = AgentId::new();
        let msgs: Vec<StateMessage> = vec![
            StateMessage::AgentHealthChanged {
                agent: a,
                health: AgentHealth::Operational,
            },
            StateMessage::MomentumChanged {
                tier: MomentumTier::Hot,
            },
            StateMessage::RallyConsumed {
                by: a,
                remaining: 2,
            },
            StateMessage::RallyReplenished { remaining: 3 },
            StateMessage::IntentAck {
                agent: a,
                intent_sequence: 1,
            },
            StateMessage::AgentLeft { agent: a },
        ];
        assert_eq!(msgs.len(), 6);
    }
}
