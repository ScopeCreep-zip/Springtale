//! Step 9a — L6 commander-override dispatch (B1).
//!
//! Per `COOPERATION.md §3.4`: when the cooperation primitives have
//! exhausted (rally tokens gone, supervision escalated, formation stuck in
//! Cold), the orchestrator's intervention layer fires one of four verbs:
//! `ChangeIntent` / `InjectFuel` / `ForcedDissolve` / `EscalateToUser`.
//!
//! This step is pure plumbing — the evaluator
//! (`orchestrator/intervention/evaluator/rules.rs`) decides; the executor
//! (`orchestrator/intervention/action/dispatcher.rs`) applies. Signals are
//! built from formation state plus the supervisor-flagged
//! `escalation_pending` and `needs_replan` fields written by
//! `tick_steps/supervision.rs` (B10) the previous tick.
//!
//! The decision sites in this pipeline that produce intervention signals:
//! - `check_cascade.rs` increments `cascade_hit_streak` on cascade detection
//! - `momentum.check_decay()` increments `cold_ticks` while in Cold tier
//! - `supervision.rs` (B10) sets `escalation_pending` / `needs_replan`
//! - rally consumption is read directly from `formation.rally.tokens`
//! - incapacitated count + operational count come from members directly

use crate::cooperation::formation::Formation;
use crate::orchestrator::intervention::{
    trait_::{InterventionAction, InterventionEvaluator},
    types::InterventionSignals,
};
use springtale_cooperation::types::AgentHealth;

use super::TickDeps;

pub async fn run(formation: &mut Formation, deps: &TickDeps<'_>) {
    let signals = build_signals(formation);

    let Some(intervention) = deps.intervention_evaluator.evaluate(&signals) else {
        return;
    };

    if let Err(e) = deps
        .intervention_action
        .execute(&intervention, formation)
        .await
    {
        tracing::error!(
            formation = %formation.id.0,
            error = %e,
            "intervention dispatch failed"
        );
    } else {
        tracing::info!(
            formation = %formation.id.0,
            ?intervention,
            "intervention applied"
        );
        // Phase H5: surface to the cooperation events stream so the
        // EventRibbon can toast the user (interventions are high-severity).
        let kind = match &intervention {
            crate::orchestrator::intervention::types::Intervention::ChangeIntent(_) => {
                springtale_cooperation::events::InterventionKind::ChangeIntent
            }
            crate::orchestrator::intervention::types::Intervention::InjectFuel(b) => {
                springtale_cooperation::events::InterventionKind::InjectFuel {
                    amount: b.remaining(),
                }
            }
            crate::orchestrator::intervention::types::Intervention::ForcedDissolve { .. } => {
                springtale_cooperation::events::InterventionKind::ForcedDissolve
            }
            crate::orchestrator::intervention::types::Intervention::EscalateToUser { .. } => {
                springtale_cooperation::events::InterventionKind::EscalateToUser
            }
        };
        springtale_cooperation::events::emit(
            deps.cooperation_tx,
            springtale_cooperation::events::CooperationEvent::InterventionFired {
                formation_id: formation.id,
                intervention: kind,
                summary: format!("{intervention:?}"),
            },
        );
    }

    // Whatever the executor did, clear the supervisor-flagged escalation
    // signal so the next tick starts fresh. `needs_replan` is cleared by
    // the L5 CBBA executor (B3) when it actually performs a replan.
    formation.escalation_pending = None;
}

fn build_signals(formation: &Formation) -> InterventionSignals {
    InterventionSignals {
        cascade_hits: formation.cascade_hit_streak,
        rally_tokens: formation.rally.tokens.remaining() as u32,
        cbba_stalled: formation.needs_replan,
        incapacitated_agents: formation
            .members
            .iter()
            .filter(|m| matches!(m.health, AgentHealth::Incapacitated))
            .count() as u32,
        operational_count: formation.operational_count() as u32,
        cold_duration_ticks: formation.momentum.cold_ticks,
        // B1 audit-fix: actually consume the supervisor-flagged
        // escalation set by `tick_steps/supervision.rs` (B10). Was
        // previously cleared without reading — silent drop of L6
        // escalations.
        escalation_reason: formation.escalation_pending.clone(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::cooperation::formation::{Formation, FormationMember};
    use springtale_cooperation::cadence::{AgentId, IntentPattern};
    use springtale_cooperation::momentum::MomentumTier;
    use springtale_cooperation::types::FormationConstraints;

    fn formation_with_one_member() -> Formation {
        let m = FormationMember::new(AgentId::new(), vec!["slack".into()]);
        Formation::new_disconnected(
            vec![m],
            IntentPattern::Execute { plan_id: None },
            FormationConstraints::default(),
        )
    }

    /// B1 audit-fix regression: `formation.escalation_pending` must
    /// surface in the signals (was being dropped silently).
    #[test]
    fn escalation_pending_propagates_to_signals() {
        let mut f = formation_with_one_member();
        f.escalation_pending = Some("rally exhausted".to_owned());
        let sig = build_signals(&f);
        assert_eq!(
            sig.escalation_reason.as_deref(),
            Some("rally exhausted"),
            "supervisor escalation must reach the L6 evaluator"
        );
    }

    /// B1 acceptance: Cold-stuck formation past threshold ticks
    /// triggers EscalateToUser per plan §B1 verification + plan
    /// "End-to-end backend check".
    #[test]
    fn cold_stuck_signal_triggers_escalate_to_user() {
        let mut f = formation_with_one_member();
        f.momentum.tier = MomentumTier::Cold;
        f.momentum.cold_ticks = 800; // past default 700-tick threshold
        let sig = build_signals(&f);
        let evaluator = crate::orchestrator::intervention::evaluator::RuleBasedEvaluator::default();
        let intervention = evaluator.evaluate(&sig).expect("intervention fires");
        assert!(matches!(
            intervention,
            crate::orchestrator::intervention::types::Intervention::EscalateToUser { .. }
        ));
    }
}
