//! L6 intervention integration test.
//!
//! Scenario: a formation has exhausted its rally budget, cascade keeps
//! firing, and the most recent CBBA replan stalled. The rule-based
//! evaluator picks `ForcedDissolve`; the default action dispatcher rewrites
//! the formation's intent to `Dissolve` so the runtime lifecycle tears it
//! down on the next sweep.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use springtale_bot::cooperation::formation::{Formation, FormationMember};
use springtale_bot::orchestrator::fuel::FuelBudget;
use springtale_bot::orchestrator::intervention::{
    action::DefaultInterventionAction,
    evaluator::{InterventionThresholds, RuleBasedEvaluator},
    trait_::{InterventionAction, InterventionEvaluator},
    types::{Intervention, InterventionSignals},
};
use springtale_cooperation::cadence::{AgentId, IntentPattern};
use springtale_cooperation::types::FormationConstraints;

fn make_formation() -> Formation {
    let members = vec![
        FormationMember::new(AgentId::new(), vec!["github".into()]),
        FormationMember::new(AgentId::new(), vec!["slack".into()]),
    ];
    let constraints = FormationConstraints {
        fuel_budget: springtale_cooperation::FuelAmount(10_000),
        ..Default::default()
    };
    Formation::new(
        members,
        IntentPattern::Execute { plan_id: None },
        constraints,
    )
}

#[tokio::test]
async fn rally_exhausted_plus_cbba_stalled_forces_dissolve() {
    let evaluator = RuleBasedEvaluator::new(InterventionThresholds::default());

    let signals = InterventionSignals {
        cascade_hits: 3,
        rally_tokens: 0,
        cbba_stalled: true,
        incapacitated_agents: 0,
        operational_count: 2,
        cold_duration_ticks: 50,
    };

    let intervention = evaluator
        .evaluate(&signals)
        .expect("evaluator should fire an intervention");
    let Intervention::ForcedDissolve { reason } = &intervention else {
        panic!("expected ForcedDissolve");
    };
    assert!(reason.contains("CBBA") || reason.contains("cascade"));

    let mut formation = make_formation();
    let action = DefaultInterventionAction;
    action
        .execute(&intervention, &mut formation)
        .await
        .expect("action should apply cleanly");

    let IntentPattern::Dissolve { reason } = &formation.intent else {
        panic!("expected Dissolve intent after forced dissolve");
    };
    assert!(!reason.is_empty());
}

#[tokio::test]
async fn healthy_formation_gets_no_intervention() {
    let evaluator = RuleBasedEvaluator::default();
    let signals = InterventionSignals {
        operational_count: 2,
        rally_tokens: 3,
        ..Default::default()
    };
    assert!(evaluator.evaluate(&signals).is_none());
}

#[tokio::test]
async fn inject_fuel_tops_up_budget() {
    let mut formation = make_formation();
    let before = formation.fuel.remaining();
    let action = DefaultInterventionAction;
    action
        .execute(
            &Intervention::InjectFuel(FuelBudget::new(5_000)),
            &mut formation,
        )
        .await
        .expect("inject");
    assert_eq!(formation.fuel.remaining(), before + 5_000);
}

#[tokio::test]
async fn change_intent_rewrites_formation_intent() {
    let mut formation = make_formation();
    let action = DefaultInterventionAction;
    let new_intent = IntentPattern::Stabilize {
        reason: "cooling down".into(),
    };
    action
        .execute(
            &Intervention::ChangeIntent(new_intent),
            &mut formation,
        )
        .await
        .expect("apply");
    let IntentPattern::Stabilize { reason } = &formation.intent else {
        panic!("expected Stabilize intent");
    };
    assert_eq!(reason, "cooling down");
}
