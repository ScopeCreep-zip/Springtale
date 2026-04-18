//! L2 react-to-peer step — drain StateSubscriber and update local awareness.
//!
//! Processes peer state messages (health changes, momentum shifts, rally
//! token consumption). Does NOT produce an action (returns None always) —
//! its purpose is to keep the agent's local awareness current so subsequent
//! steps (scan, bid) operate on fresh data.

use crate::authority;
use crate::awareness::LocalAwareness;
use crate::dissemination::StateMessage;
use crate::layer::LayerId;
use crate::momentum::MomentumTier;

/// Drain any pending `StateMessage`s from the subscription and apply them
/// to the agent's `LocalAwareness`. Always returns `None` — this step
/// mutates state but doesn't produce an action.
pub fn step_react(
    messages: &[StateMessage],
    awareness: &mut LocalAwareness,
    tier: MomentumTier,
) {
    if !authority::allows(tier, LayerId::L2State) {
        return;
    }

    for msg in messages {
        match msg {
            StateMessage::MomentumChanged { tier } => {
                awareness.formation_momentum = *tier;
            }
            StateMessage::AgentHealthChanged { agent, health } => {
                if let Some(neighbor) = awareness.neighbor_states.get_mut(agent) {
                    neighbor.health = health.clone();
                }
            }
            StateMessage::AgentLeft { agent } => {
                awareness.neighbor_states.remove(agent);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::awareness::NeighborSnapshot;
    use crate::cadence::AgentId;
    use crate::types::AgentHealth;

    fn awareness_with_neighbor(agent: AgentId) -> LocalAwareness {
        let mut a = LocalAwareness::default();
        a.update_neighbor(NeighborSnapshot {
            agent_id: agent,
            health: AgentHealth::Operational,
            role_name: "General".to_owned(),
            fuel_remaining_pct: 1.0,
            last_action_success: true,
            attention_load: 0.0,
            liveness: crate::supervision::Liveness::Alive,
            last_updated: Instant::now(),
        });
        a
    }

    #[test]
    fn momentum_change_updates_awareness() {
        let mut awareness = LocalAwareness::default();
        let msgs = vec![StateMessage::MomentumChanged {
            tier: MomentumTier::Fever,
        }];
        step_react(&msgs, &mut awareness, MomentumTier::Warming);
        assert_eq!(awareness.formation_momentum, MomentumTier::Fever);
    }

    #[test]
    fn agent_left_removes_from_neighbors() {
        let agent = AgentId::new();
        let mut awareness = awareness_with_neighbor(agent);
        assert!(awareness.neighbor_states.contains_key(&agent));
        step_react(
            &[StateMessage::AgentLeft { agent }],
            &mut awareness,
            MomentumTier::Hot,
        );
        assert!(!awareness.neighbor_states.contains_key(&agent));
    }

    #[test]
    fn cold_tier_skips_all_messages() {
        let agent = AgentId::new();
        let mut awareness = awareness_with_neighbor(agent);
        step_react(
            &[StateMessage::AgentLeft { agent }],
            &mut awareness,
            MomentumTier::Cold,
        );
        assert!(
            awareness.neighbor_states.contains_key(&agent),
            "Cold should not process L2 messages"
        );
    }

    #[test]
    fn health_change_updates_neighbor() {
        let agent = AgentId::new();
        let mut awareness = awareness_with_neighbor(agent);
        step_react(
            &[StateMessage::AgentHealthChanged {
                agent,
                health: AgentHealth::Incapacitated,
            }],
            &mut awareness,
            MomentumTier::Hot,
        );
        assert!(matches!(
            awareness.neighbor_states.get(&agent).unwrap().health,
            AgentHealth::Incapacitated
        ));
    }
}
