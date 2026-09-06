//! Step 9b — recovery evaluation (`COOPERATION.md §18`).
//!
//! When agents are distressed, nearby operational agents evaluate whether
//! to help. Per L4D "rescue pinned survivors" the highest priority is
//! reviving incapacitated members. Each volunteering helper runs the
//! big-brain–style utility scorer in `recovery::executor::evaluate_recovery`
//! and the first willing helper takes the work (Monster Hunter "nearest
//! hunter").
//!
//! Decisions are collected first (immutable iteration over members) then
//! applied (mutable rewrite of target health) so the borrow checker stays
//! out of the way.

use std::time::Duration;
use tokio::sync::broadcast;

use crate::cooperation::formation::Formation;
use springtale_cooperation::cadence::AgentId;
use springtale_cooperation::comms::{BroadcastTrigger, StateBroadcastMsg, StateMessage};
use springtale_cooperation::events::{self, CooperationEvent, CooperationEventEnvelope};
use springtale_cooperation::recovery::{DistressSignal, RecoveryAction, executor as recovery_exec};
use springtale_cooperation::sacrifice::scorer::FormationSnapshot;
use springtale_cooperation::types::AgentHealth;

pub fn run(
    formation: &mut Formation,
    cooperation_tx: Option<&broadcast::Sender<CooperationEventEnvelope>>,
) {
    let distress_signals = build_distress_signals(formation);
    if distress_signals.is_empty() {
        return;
    }

    let snapshot = build_snapshot(formation);
    let decisions = collect_decisions(formation, &distress_signals, &snapshot, cooperation_tx);
    apply_recovery_decisions(formation, decisions, cooperation_tx);
}

fn build_distress_signals(formation: &Formation) -> Vec<DistressSignal> {
    formation
        .members
        .iter()
        .filter_map(|m| match &m.health {
            AgentHealth::Degraded { recovery_count } => Some(DistressSignal::HealthLow {
                agent_id: m.agent_id,
                health_pct: 1.0 - (*recovery_count as f32 * 0.3).min(0.9),
            }),
            AgentHealth::Incapacitated => Some(DistressSignal::Incapacitated {
                agent_id: m.agent_id,
                bleedout_remaining: Duration::from_secs(30),
            }),
            AgentHealth::Dead { recoverable } => Some(DistressSignal::Dead {
                agent_id: m.agent_id,
                recoverable: *recoverable,
            }),
            _ => None,
        })
        .collect()
}

fn build_snapshot(formation: &Formation) -> FormationSnapshot {
    FormationSnapshot {
        member_count: formation.members.len(),
        operational_count: formation.operational_count(),
        momentum_tier: formation.momentum.tier,
        rally_tokens: formation.rally.tokens.remaining() as u32,
        capabilities: formation
            .members
            .iter()
            .flat_map(|m| m.capabilities.iter().cloned())
            .collect(),
        unique_capabilities: vec![],
    }
}

fn collect_decisions(
    formation: &Formation,
    distress_signals: &[DistressSignal],
    snapshot: &FormationSnapshot,
    cooperation_tx: Option<&broadcast::Sender<CooperationEventEnvelope>>,
) -> Vec<(AgentId, AgentId, RecoveryAction)> {
    let mut decisions = Vec::new();
    for signal in distress_signals {
        for member in &formation.members {
            if !member.is_operational() {
                continue;
            }
            let attention_snapshot = formation.attention_broker.current();
            let eval = recovery_exec::evaluate_recovery(
                member.agent_id,
                &member.capabilities,
                attention_snapshot.load(&member.agent_id),
                signal,
                snapshot,
                &member.awareness,
                &attention_snapshot,
            );
            if eval.should_help {
                if let Some(recovery_action) = eval.action {
                    let target_id = match signal {
                        DistressSignal::HealthLow { agent_id, .. }
                        | DistressSignal::Incapacitated { agent_id, .. }
                        | DistressSignal::Dead { agent_id, .. }
                        | DistressSignal::Degraded { agent_id, .. } => *agent_id,
                    };
                    tracing::info!(
                        formation = %formation.id.0,
                        helper = %member.agent_id.0,
                        target = %target_id.0,
                        help_utility = eval.help_utility,
                        kind = ?recovery_action.kind(),
                        "agent volunteering for recovery"
                    );
                    events::emit(
                        cooperation_tx,
                        CooperationEvent::RecoveryActionTaken {
                            formation_id: formation.id,
                            helper: member.agent_id,
                            in_distress: target_id,
                            action: format!("{:?}", recovery_action.kind()),
                        },
                    );
                    decisions.push((member.agent_id, target_id, recovery_action));
                }
                break; // first willing helper takes it
            }
        }
    }
    decisions
}

/// Apply the collected recovery decisions. Extracted so the §18.2 fragility
/// FSM can be unit-tested without spinning up a full `Bot` (see the test
/// module at the bottom).
/// Apply `(helper, target, action)` decisions. The helper says `Helping`
/// (plan §1.15) so peers and the canvas see who is reviving whom.
pub fn apply_recovery_decisions(
    formation: &mut Formation,
    decisions: Vec<(AgentId, AgentId, RecoveryAction)>,
    cooperation_tx: Option<&broadcast::Sender<CooperationEventEnvelope>>,
) {
    let formation_id_str = formation.id.0.to_string();
    for (helper, target_id, action) in decisions {
        springtale_cooperation::utterance::utter(
            &mut formation.utter_ctx(cooperation_tx),
            Some(helper),
            springtale_cooperation::UtteranceKind::Helping { target: target_id },
        );
        let transition = {
            let Some(target) = formation.member_mut(&target_id) else {
                continue;
            };
            let before = target.health.clone();
            let after = action.apply(before.clone());
            target.health = after.clone();
            (before, after)
        };
        let (before, after) = transition;

        let same_kind = std::mem::discriminant(&before) == std::mem::discriminant(&after);
        if same_kind && matches!(before, AgentHealth::Degraded { .. }) {
            tracing::debug!(
                formation = %formation_id_str,
                target = %target_id.0,
                before = ?before,
                after = ?after,
                "recovery applied (counter tick)"
            );
        } else if !same_kind {
            tracing::info!(
                formation = %formation_id_str,
                target = %target_id.0,
                before = ?before,
                after = ?after,
                "recovery transitioned agent health"
            );
        }

        if matches!(after, AgentHealth::Dead { .. }) {
            formation.bus.broadcast_state(StateBroadcastMsg {
                source: target_id,
                trigger: BroadcastTrigger::AgentDown(target_id),
                message: StateMessage {
                    content: "died after escalating fragility".to_owned(),
                    severity: 1.0,
                },
            });
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::cooperation::formation::{Formation, FormationMember};
    use springtale_cooperation::cadence::{AgentId, IntentPattern};
    use springtale_cooperation::types::{AgentHealth, FormationConstraints};

    fn setup_formation_with_member(health: AgentHealth) -> (Formation, AgentId) {
        let mut member = FormationMember::new(AgentId::new(), vec!["slack".into()]);
        let target_id = member.agent_id;
        member.health = health;
        let formation = Formation::new_disconnected(
            vec![member],
            IntentPattern::Execute { plan_id: None },
            FormationConstraints {
                fuel_budget: springtale_cooperation::FuelAmount(1000),
                ..Default::default()
            },
        );
        (formation, target_id)
    }

    #[tokio::test]
    async fn apply_recovery_quick_fix_bumps_counter() {
        let (mut formation, target_id) =
            setup_formation_with_member(AgentHealth::Degraded { recovery_count: 1 });
        let helper = AgentId::new();
        let action = RecoveryAction::PeerRevive {
            healer: helper,
            target: target_id,
            duration: Duration::from_secs(5),
            healer_vulnerability: 0.5,
        };
        apply_recovery_decisions(&mut formation, vec![(target_id, target_id, action)], None);
        let after = &formation.member(&target_id).unwrap().health;
        assert!(
            matches!(after, AgentHealth::Degraded { recovery_count: 2 }),
            "expected Degraded{{2}}, got {after:?}"
        );
    }

    #[tokio::test]
    async fn apply_recovery_third_quick_fix_kills_and_broadcasts() {
        let (mut formation, target_id) =
            setup_formation_with_member(AgentHealth::Degraded { recovery_count: 2 });
        let mut state_rx = formation.bus.subscribe(AgentId::new()).state_rx;
        let helper = AgentId::new();
        let action = RecoveryAction::PeerRevive {
            healer: helper,
            target: target_id,
            duration: Duration::from_secs(5),
            healer_vulnerability: 0.5,
        };
        apply_recovery_decisions(&mut formation, vec![(helper, target_id, action)], None);
        let after = &formation.member(&target_id).unwrap().health;
        assert!(
            matches!(after, AgentHealth::Dead { recoverable: true }),
            "expected Dead{{recoverable:true}}, got {after:?}"
        );
        // Plan §1.15: the helper says `Helping` first (Speech carrier).
        let said = state_rx.try_recv().expect("Helping utterance");
        assert!(matches!(
            said.trigger,
            BroadcastTrigger::Utterance(ref u)
                if u.agent == Some(helper)
                    && u.utterance == springtale_cooperation::UtteranceKind::Helping { target: target_id }
        ));
        let msg = state_rx.try_recv().expect("AgentDown broadcast");
        assert!(matches!(
            msg.trigger,
            BroadcastTrigger::AgentDown(id) if id == target_id
        ));
    }

    #[tokio::test]
    async fn apply_recovery_environmental_restores_operational() {
        let (mut formation, target_id) =
            setup_formation_with_member(AgentHealth::Degraded { recovery_count: 2 });
        let mut state_rx = formation.bus.subscribe(AgentId::new()).state_rx;
        let action = RecoveryAction::EnvironmentalRecovery {
            source_resource: springtale_cooperation::types::ResourceId::from("well-1"),
            beneficiary: target_id,
            depletes_resource: false,
        };
        apply_recovery_decisions(&mut formation, vec![(target_id, target_id, action)], None);
        assert!(matches!(
            formation.member(&target_id).unwrap().health,
            AgentHealth::Operational
        ));
        let mut heard = Vec::new();
        while let Ok(msg) = state_rx.try_recv() {
            heard.push(msg.trigger);
        }
        assert!(
            !heard
                .iter()
                .any(|t| matches!(t, BroadcastTrigger::AgentDown(_))),
            "proper recovery should not broadcast AgentDown, got {heard:?}"
        );
        assert!(
            heard
                .iter()
                .any(|t| matches!(t, BroadcastTrigger::Utterance(_))),
            "the helper says Helping"
        );
    }

    #[tokio::test]
    async fn apply_recovery_ignores_unknown_target() {
        let (mut formation, target_id) = setup_formation_with_member(AgentHealth::Operational);
        let ghost_id = AgentId::new();
        let action = RecoveryAction::PeerRevive {
            healer: AgentId::new(),
            target: ghost_id,
            duration: Duration::from_secs(5),
            healer_vulnerability: 0.5,
        };
        apply_recovery_decisions(&mut formation, vec![(ghost_id, ghost_id, action)], None);
        assert!(matches!(
            formation.member(&target_id).unwrap().health,
            AgentHealth::Operational
        ));
    }
}
