//! Intent — the §3.2 orchestration chokepoint for intent changes.
//!
//! Per COOPERATION.pdf §3.2:
//! Game source: Patapon drum patterns, Total War attack/defend orders,
//! Siege IGL strat calls.
//!
//! "Intent describes WHAT, never HOW. 'Attack' tells the formation to
//! engage. It does not tell individual agents which target to pick,
//! what timing to use, or what sequence to follow."
//!
//! Three sources for intent transitions (§5.5) — ALL of them route
//! through [`apply_intent`] so every intent write is audited in one
//! place and the §7 momentum FSM always observes the change:
//!
//! 1. Orchestrator/user command (`FormationCommand::ChangeIntent`)
//! 2. Formation self-governance — a consensus-approved
//!    `DecisionSubject::IntentChange` at Fever tier
//!    (`tick_steps/resolve_consensus`)
//! 3. L6 intervention (`Intervention::ChangeIntent`)
//!
//! The intent travels to members on the `FormationContext` watch
//! channel (§6) — the cadence bus is a pure metronome and carries no
//! intent (see `springtale_cooperation::cadence`).

use crate::cooperation::cadence::IntentPattern;
use crate::cooperation::formation::Formation;
use springtale_cooperation::momentum::MomentumEvent;

/// Replace the formation's intent and rebroadcast its context.
///
/// Side effects, in order:
/// 1. Momentum observes `IntentChanged` — the consecutive-success run
///    resets (§7: coherence is earned per-intent, not carried across).
/// 2. `formation.intent` is replaced.
/// 3. The `FormationContext` watch channel rebroadcasts so every member
///    sees the new intent on its next `changed()` (§6).
pub fn apply_intent(formation: &mut Formation, intent: IntentPattern) {
    formation
        .momentum
        .apply_event(&MomentumEvent::IntentChanged(intent.clone()));
    formation.intent = intent;
    formation.broadcast_context();
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::cooperation::formation::FormationMember;
    use crate::cooperation::types::FormationConstraints;
    use springtale_cooperation::cadence::AgentId;

    fn formation() -> Formation {
        Formation::new_disconnected(
            vec![FormationMember::new(AgentId::new(), vec!["github".into()])],
            IntentPattern::Execute { plan_id: None },
            FormationConstraints::default(),
        )
    }

    #[test]
    fn apply_intent_swaps_and_resets_momentum_run() {
        let mut f = formation();
        for _ in 0..2 {
            f.momentum.record_success();
        }
        assert_eq!(f.momentum.consecutive_successes, 2);

        apply_intent(
            &mut f,
            IntentPattern::Surge {
                objective: "ship it".into(),
            },
        );

        assert!(matches!(f.intent, IntentPattern::Surge { .. }));
        assert_eq!(
            f.momentum.consecutive_successes, 0,
            "intent change resets the §7 coherence run"
        );
    }

    #[test]
    fn apply_intent_rebroadcasts_context() {
        let mut f = formation();
        let (_, mut ctx_rx) = f.subscribe();
        ctx_rx.borrow_and_update();

        apply_intent(
            &mut f,
            IntentPattern::Stabilize {
                reason: "cooling".into(),
            },
        );

        assert!(
            ctx_rx.has_changed().unwrap(),
            "watch channel observed the intent change"
        );
        assert!(matches!(
            ctx_rx.borrow().intent,
            IntentPattern::Stabilize { .. }
        ));
    }
}
