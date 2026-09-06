//! L2 react-to-peer step — drain a `StateSubscriber` and fold each message
//! into `LocalAwareness` (`COOPERATION.md §19` Overcooked-style implicit
//! signals). Never produces an action; the next steps in `AgentLoop::tick`
//! see fresh awareness.
//!
//! Trait-bounded per plan §A2 — `&mut dyn StateSubscriber` so any
//! receiver impl plugs in (formation bus subscription, in-test mock).

use crate::authority;
use crate::awareness::LocalAwareness;
use crate::dissemination::StateMessage;
use crate::dissemination::trait_::StateSubscriber;
use crate::layer::LayerId;
use crate::momentum::MomentumTier;
use crate::supervision::Liveness;
use crate::utterance::UtteranceKind;

pub fn run(bus: &mut dyn StateSubscriber, awareness: &mut LocalAwareness, tier: MomentumTier) {
    if !authority::allows(tier, LayerId::L2State) {
        return;
    }
    while let Some(msg) = bus.try_recv() {
        apply(awareness, msg);
    }
}

fn apply(awareness: &mut LocalAwareness, msg: StateMessage) {
    match msg {
        StateMessage::MomentumChanged { tier } => {
            awareness.formation_momentum = tier;
        }
        StateMessage::AgentHealthChanged { agent, health } => {
            if let Some(neighbor) = awareness.neighbor_states.get_mut(&agent) {
                neighbor.health = health;
            }
        }
        StateMessage::AgentLeft { agent } => {
            awareness.neighbor_states.remove(&agent);
        }
        // Cohn (2013): speech and burst are heard by others in the scene;
        // a thought bubble is private to the speaker and the observer.
        StateMessage::Utterance(u) if u.carrier.heard_by_peers() => {
            let Some(agent) = u.agent else { return };
            if let Some(n) = awareness.neighbor_states.get_mut(&agent) {
                match u.utterance {
                    UtteranceKind::Failed => n.last_action_success = false,
                    UtteranceKind::Working
                    | UtteranceKind::Firing
                    | UtteranceKind::Claimed { .. } => n.last_action_success = true,
                    UtteranceKind::Down => n.liveness = Liveness::Down { since_tick: u.seq },
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::awareness::NeighborSnapshot;
    use crate::cadence::AgentId;
    use crate::types::AgentHealth;
    use std::collections::VecDeque;
    use std::time::Instant;

    /// Tiny in-memory `StateSubscriber` for tests — pops from a deque.
    struct VecBus {
        msgs: VecDeque<StateMessage>,
    }
    impl StateSubscriber for VecBus {
        fn try_recv(&mut self) -> Option<StateMessage> {
            self.msgs.pop_front()
        }
    }

    fn awareness_with_neighbor(agent: AgentId) -> LocalAwareness {
        let mut a = LocalAwareness::default();
        a.update_neighbor(NeighborSnapshot {
            agent_id: agent,
            health: AgentHealth::Operational,
            role: crate::awareness::RoleSignature::General,
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
        let mut bus = VecBus {
            msgs: vec![StateMessage::MomentumChanged {
                tier: MomentumTier::Fever,
            }]
            .into(),
        };
        let mut a = LocalAwareness::default();
        run(&mut bus, &mut a, MomentumTier::Warming);
        assert_eq!(a.formation_momentum, MomentumTier::Fever);
    }

    #[test]
    fn cold_tier_skips_messages() {
        let agent = AgentId::new();
        let mut bus = VecBus {
            msgs: vec![StateMessage::AgentLeft { agent }].into(),
        };
        let mut a = awareness_with_neighbor(agent);
        run(&mut bus, &mut a, MomentumTier::Cold);
        assert!(a.neighbor_states.contains_key(&agent));
    }

    fn utterance(
        agent: AgentId,
        kind: UtteranceKind,
        carrier: crate::utterance::Carrier,
    ) -> StateMessage {
        StateMessage::Utterance(crate::utterance::Utterance {
            formation_id: None,
            agent: Some(agent),
            rule_id: None,
            utterance: kind,
            carrier,
            shape: crate::utterance::Shape::Circle,
            tone: crate::utterance::Tone::Urgent,
            seq: crate::tick::TickId(7),
            ttl_ticks: 3,
            glyph_frames: vec!["@#!?".to_owned()],
            mirror_rtl: false,
            label_key: "utter.failed".to_owned(),
        })
    }

    #[test]
    fn test_apply_speech_utterance_from_a_lands_in_b_snapshot() {
        let a = AgentId(uuid::Uuid::new_v4());
        let mut awareness = awareness_with_neighbor(a);
        let mut bus = VecBus {
            msgs: vec![utterance(
                a,
                UtteranceKind::Failed,
                crate::utterance::Carrier::Burst,
            )]
            .into(),
        };
        run(&mut bus, &mut awareness, MomentumTier::Warming);
        assert!(!awareness.neighbor_states[&a].last_action_success);

        let mut bus = VecBus {
            msgs: vec![utterance(
                a,
                UtteranceKind::Down,
                crate::utterance::Carrier::Speech,
            )]
            .into(),
        };
        run(&mut bus, &mut awareness, MomentumTier::Warming);
        assert!(matches!(
            awareness.neighbor_states[&a].liveness,
            Liveness::Down { since_tick } if since_tick == crate::tick::TickId(7)
        ));
    }

    #[test]
    fn test_apply_thought_utterance_is_private_and_ignored() {
        let a = AgentId(uuid::Uuid::new_v4());
        let mut awareness = awareness_with_neighbor(a);
        let mut bus = VecBus {
            msgs: vec![
                utterance(a, UtteranceKind::Failed, crate::utterance::Carrier::Thought),
                utterance(a, UtteranceKind::Down, crate::utterance::Carrier::None),
            ]
            .into(),
        };
        run(&mut bus, &mut awareness, MomentumTier::Warming);
        assert!(awareness.neighbor_states[&a].last_action_success);
        assert!(matches!(
            awareness.neighbor_states[&a].liveness,
            Liveness::Alive
        ));
    }
}
