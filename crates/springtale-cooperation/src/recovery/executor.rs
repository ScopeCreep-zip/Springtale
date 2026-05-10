//! Recovery action selection — §18's 5-part decision framework.
//!
//! When an agent is in distress, neighboring agents evaluate locally:
//! 1. Can I help? (capability check)
//! 2. Should I help? (cost vs value — Army of Two dilemma)
//! 3. Should someone else help? (proximity — MH nearest hunter)
//! 4. Should we prevent instead? (Patapon defend, DRG shield)
//! 5. Should we let them transform? (Siege dead→intel)
//!
//! Uses the utility scoring framework (§24 pattern) for the "should"
//! questions. The "can" question is a binary filter.

use crate::attention::AttentionEconomy;
use crate::awareness::LocalAwareness;
use crate::cadence::AgentId;
use crate::capability::CapabilityDecl;
use crate::recovery::{DistressSignal, RecoveryAction};
use crate::sacrifice::scorer::FormationSnapshot;
use crate::utility::evaluator::{Linear, ResponseCurve, Sigmoid};
use crate::utility::picker::{Highest, Picker};

/// Result of recovery evaluation for a potential helper.
pub struct RecoveryEvaluation {
    /// Whether this agent should attempt recovery.
    pub should_help: bool,
    /// The recommended recovery action (if should_help).
    pub action: Option<RecoveryAction>,
    /// Utility score for helping (0.0-1.0).
    pub help_utility: f32,
    /// Utility score for letting them transform instead.
    pub transform_utility: f32,
}

/// Evaluate whether a helper agent should recover a distressed agent.
///
/// Per §18.4: "This evaluation is the big-brain utility AI pattern.
/// Decision is local through the awareness system, NOT centrally
/// through the orchestrator."
pub fn evaluate_recovery(
    helper: AgentId,
    helper_capabilities: &[CapabilityDecl],
    helper_load: f32,
    distress: &DistressSignal,
    formation: &FormationSnapshot,
    awareness: &LocalAwareness,
    attention: &AttentionEconomy,
) -> RecoveryEvaluation {
    let distressed_id = match distress {
        DistressSignal::HealthLow { agent_id, .. } => *agent_id,
        DistressSignal::Incapacitated { agent_id, .. } => *agent_id,
        DistressSignal::Dead { agent_id, .. } => *agent_id,
        DistressSignal::Degraded { agent_id, .. } => *agent_id,
    };

    // 1. Can I help? (binary capability filter)
    //    Per DF: check labor enabled + tool available
    //    Also: can't help unrecoverable agents
    let is_unrecoverable = matches!(distress, DistressSignal::Dead { recoverable: false, .. });
    let can_help = !helper_capabilities.is_empty()
        && helper != distressed_id
        && !is_unrecoverable;

    if !can_help {
        return RecoveryEvaluation {
            should_help: false,
            action: None,
            help_utility: 0.0,
            transform_utility: 0.0,
        };
    }

    // 2. Should I help? (cost vs value — Army of Two dilemma)
    //    "Is saving them worth more than continuing my current task?"
    //    Factor in: how critical was the distressed agent's work?
    //    Higher attention load on distressed = more valuable to recover.
    let benefit_to_distressed = match distress {
        DistressSignal::HealthLow { health_pct, .. } => 1.0 - health_pct,
        DistressSignal::Incapacitated { .. } => 0.9,
        DistressSignal::Dead { recoverable: true, .. } => 0.7,
        DistressSignal::Dead { recoverable: false, .. } => 0.0,
        DistressSignal::Degraded { .. } => 0.5,
    };
    let distressed_value = attention.load(&distressed_id); // how much work they were doing
    let cost_to_helper = helper_load; // busy helpers pay more
    let raw_should = (benefit_to_distressed + distressed_value * 0.3) - cost_to_helper * 0.5;

    let should_curve = Sigmoid { midpoint: 0.3, steepness: 6.0 };
    let should_score = should_curve.evaluate((raw_should + 1.0) / 2.0);

    // 3. Should someone else help? (proximity scoring)
    //    Check if any neighbor has lower load (= more capacity to help).
    //    If someone else is clearly better positioned, yield to them.
    let someone_better = awareness.neighbor_states.values().any(|n| {
        n.attention_load < helper_load * 0.7
            && matches!(n.health, crate::types::AgentHealth::Operational)
    });
    let proximity_penalty = if someone_better { 0.3 } else { 0.0 };

    // 4. Should we prevent instead? (defensive posture)
    //    Per Patapon defend rhythm: sometimes preventing further damage
    //    is better than recovering from existing damage.
    let prevention_utility = if formation.operational_count > 2 {
        0.3 // enough members to absorb — prevention is moderate value
    } else {
        0.1 // too few members — recovery is more urgent than prevention
    };

    // 5. Should we let them transform? (Siege dead→intel)
    //    If the agent is dead but recoverable, AND the formation has
    //    enough members, transformation to information agent may be
    //    more valuable than revival.
    let transform_utility = match distress {
        DistressSignal::Dead { recoverable: true, .. } => {
            let info_value = Linear { min: 2.0, max: 5.0 };
            info_value.evaluate(formation.operational_count as f32) * 0.6
        }
        DistressSignal::Incapacitated { .. } => 0.2,
        _ => 0.0,
    };

    // Final help utility: should_score minus proximity penalty
    let help_utility = (should_score - proximity_penalty).max(0.0);

    // Pick: help vs transform
    let picker = Highest;
    let options = [(0usize, help_utility), (1, transform_utility), (2, prevention_utility)];
    let choice = picker.pick(&options);

    let (should_help, action) = match choice {
        Some(0) if help_utility > 0.4 => {
            // Help — choose recovery action based on distress type
            let action = match distress {
                DistressSignal::HealthLow { agent_id, .. } => Some(RecoveryAction::ByproductRecovery {
                    source: helper,
                    beneficiaries: vec![*agent_id],
                    recovery_amount: 0.3,
                    primary_action: crate::cadence::ActionDescriptor {
                        kind: "assist".to_owned(),
                        target: None,
                        payload_hash: 0,
                    },
                }),
                DistressSignal::Incapacitated { agent_id, .. } => Some(RecoveryAction::PeerRevive {
                    healer: helper,
                    target: *agent_id,
                    duration: std::time::Duration::from_secs(5),
                    healer_vulnerability: 0.6,
                }),
                DistressSignal::Dead { agent_id, recoverable: true, .. } => Some(RecoveryAction::Redeployment {
                    dead_agent: *agent_id,
                    replacement_capabilities: vec![],
                    cost: crate::recovery::RecoveryCost::SharedFuel(crate::types::FuelAmount(500)),
                    degraded: true,
                }),
                _ => None,
            };
            (true, action)
        }
        Some(1) if transform_utility > 0.3 => {
            // Let them transform — don't help, return false
            (false, None)
        }
        _ => (false, None),
    };

    RecoveryEvaluation {
        should_help,
        action,
        help_utility,
        transform_utility,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::momentum::MomentumTier;

    fn make_snapshot(operational: usize) -> FormationSnapshot {
        FormationSnapshot {
            member_count: operational,
            operational_count: operational,
            momentum_tier: MomentumTier::Warming,
            rally_tokens: 2,
            capabilities: vec![],
            unique_capabilities: vec![],
        }
    }

    fn cap(s: &str) -> CapabilityDecl {
        CapabilityDecl::new(s)
    }

    #[test]
    fn test_cant_help_self() {
        let agent = AgentId::new();
        let distress = DistressSignal::HealthLow { agent_id: agent, health_pct: 0.2 };
        let eval = evaluate_recovery(
            agent, &[cap("slack")], 0.3, &distress,
            &make_snapshot(3), &LocalAwareness::default(), &AttentionEconomy::new(&[agent]),
        );
        assert!(!eval.should_help);
    }

    #[test]
    fn test_help_low_health_neighbor() {
        let helper = AgentId::new();
        let distressed = AgentId::new();
        let distress = DistressSignal::HealthLow { agent_id: distressed, health_pct: 0.2 };
        let eval = evaluate_recovery(
            helper, &[cap("slack")], 0.1, &distress,
            &make_snapshot(4), &LocalAwareness::default(),
            &AttentionEconomy::new(&[helper, distressed]),
        );
        assert!(eval.help_utility > 0.3);
    }

    #[test]
    fn test_busy_helper_less_likely() {
        let helper = AgentId::new();
        let distressed = AgentId::new();
        let distress = DistressSignal::HealthLow { agent_id: distressed, health_pct: 0.3 };

        let eval_idle = evaluate_recovery(
            helper, &[cap("slack")], 0.1, &distress,
            &make_snapshot(4), &LocalAwareness::default(),
            &AttentionEconomy::new(&[helper, distressed]),
        );
        let eval_busy = evaluate_recovery(
            helper, &[cap("slack")], 0.9, &distress,
            &make_snapshot(4), &LocalAwareness::default(),
            &AttentionEconomy::new(&[helper, distressed]),
        );

        assert!(eval_idle.help_utility > eval_busy.help_utility,
            "idle ({}) should score higher than busy ({})",
            eval_idle.help_utility, eval_busy.help_utility);
    }

    #[test]
    fn test_dead_unrecoverable_no_help() {
        let helper = AgentId::new();
        let dead = AgentId::new();
        let distress = DistressSignal::Dead { agent_id: dead, recoverable: false };
        let eval = evaluate_recovery(
            helper, &[cap("slack")], 0.1, &distress,
            &make_snapshot(4), &LocalAwareness::default(),
            &AttentionEconomy::new(&[helper, dead]),
        );
        assert!(!eval.should_help);
    }

    #[test]
    fn test_transform_high_when_enough_members() {
        let helper = AgentId::new();
        let dead = AgentId::new();
        let distress = DistressSignal::Dead { agent_id: dead, recoverable: true };
        let eval = evaluate_recovery(
            helper, &[cap("slack")], 0.5, &distress,
            &make_snapshot(5), &LocalAwareness::default(),
            &AttentionEconomy::new(&[helper, dead]),
        );
        // With 5 members and dead recoverable, transform should be considered
        assert!(eval.transform_utility > 0.2);
    }
}
