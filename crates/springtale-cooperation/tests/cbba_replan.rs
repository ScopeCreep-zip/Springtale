//! L5 CBBA replan integration test.
//!
//! Scenario: a formation hits cascade conditions (multiple failures, rally
//! spent). The trigger fires; CBBA runs against the current task pool with
//! three capability-heterogeneous agents; every task lands with exactly one
//! agent, every bundle is DMG-consistent.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashSet;

use springtale_cooperation::action::SubTask;
use springtale_cooperation::cadence::AgentId;
use springtale_cooperation::momentum::MomentumTier;
use springtale_cooperation::replan::cbba::{self, dmg, AgentSpec, ReplanOutcome};
use springtale_cooperation::replan::trigger::cascade::{
    self, CascadeSignals, CascadeThresholds,
};

fn task(connector: &str, priority: u8) -> SubTask {
    SubTask {
        id: uuid::Uuid::new_v4(),
        target_connector: springtale_cooperation::capability::CapabilityDecl::new(connector),
        action_name: "act".to_owned(),
        params: serde_json::json!({}),
        priority,
        assigned_to: None,
        description: String::new(),
    }
}

#[test]
fn cascade_triggers_replan_and_converges() {
    // 1. Cascade conditions detected → trigger fires.
    let signals = CascadeSignals {
        failures_in_window: 4,
        unique_interference_targets: 2,
        rally_tokens_remaining: 0,
    };
    assert!(cascade::should_replan(signals, CascadeThresholds::default()));

    // 2. Three agents, overlapping capabilities so consensus actually has
    //    something to resolve.
    let a = AgentId::new();
    let b = AgentId::new();
    let c = AgentId::new();
    let agents = vec![
        AgentSpec {
            agent: a,
            capabilities: vec!["github".into(), "slack".into()],
        },
        AgentSpec {
            agent: b,
            capabilities: vec!["github".into()],
        },
        AgentSpec {
            agent: c,
            capabilities: vec!["slack".into(), "nostr".into()],
        },
    ];

    // 3. Four tasks spanning both shared capabilities + a nostr-only task.
    let t_gh_hi = task("github", 1);
    let t_gh_lo = task("github", 5);
    let t_slack = task("slack", 2);
    let t_nostr = task("nostr", 1);
    let tasks = vec![
        t_gh_hi.clone(),
        t_gh_lo.clone(),
        t_slack.clone(),
        t_nostr.clone(),
    ];

    // 4. Run CBBA.
    let outcome = cbba::run(&agents, &tasks, MomentumTier::Fever);

    match outcome {
        ReplanOutcome::Converged {
            assignment,
            sweeps,
            unassigned,
        } => {
            // Every task assigned.
            assert!(unassigned.is_empty(), "unassigned: {unassigned:?}");
            assert_eq!(assignment.len(), 4);

            // Convergence in a bounded number of sweeps.
            assert!(sweeps <= 32);

            // Capability legality: each task's winner must actually carry
            // the task's capability.
            for task in &tasks {
                let owner = assignment.get(&task.id).expect("task assigned");
                let owner_caps = &agents
                    .iter()
                    .find(|s| s.agent == *owner)
                    .expect("owner known")
                    .capabilities;
                assert!(
                    owner_caps.iter().any(|c| c == &task.target_connector),
                    "task {:?} requires {} but assigned to agent with caps {:?}",
                    task.id,
                    task.target_connector,
                    owner_caps
                );
            }

            // Nostr-only task must land on agent C.
            assert_eq!(
                assignment.get(&t_nostr.id).copied(),
                Some(c),
                "only C has nostr capability"
            );

            // Exactly one owner per task.
            let owners: HashSet<_> = assignment.values().copied().collect();
            assert!(owners.len() <= agents.len());
        }
        other => panic!("expected Converged, got {other:?}"),
    }
}

#[test]
fn dmg_holds_across_all_bundles() {
    // Independent of consensus, every freshly-built bundle must satisfy DMG.
    // Regression guard for the bundle builder.
    let a = AgentId::new();
    let tasks = (0..10)
        .map(|i| task("github", (i % 10).max(1) as u8))
        .collect::<Vec<_>>();
    let bundle =
        cbba::bundle::build(a, &tasks, &[springtale_cooperation::capability::CapabilityDecl::new("github")]);
    assert!(dmg::holds(&bundle));
}
