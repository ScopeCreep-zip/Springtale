//! End-to-end §11 consensus round-trip through the REAL executor +
//! resolution path (plan verification: "confirm a RequireConsensus
//! action round-trips propose → approve → execute"):
//!
//! 1. Formation earns Fever momentum and Peak pacing.
//! 2. Executor call #1 on a destructive task → opens a consensus
//!    proposal instead of executing (B7 gate).
//! 3. Executor call #2 → pending-vote guard: no duplicate proposal.
//! 4. All members approve → `resolve_consensus` mints the one-shot permit.
//! 5. Executor call #3 → permit consumed, the connector runs EXACTLY
//!    once, result posted to the blackboard.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use springtale_cooperation::action::SubTask;
use springtale_cooperation::cadence::{AgentId, IntentPattern};
use springtale_cooperation::consensus::{DecisionDescriptor, DecisionSubject, VoteChoice};
use springtale_cooperation::momentum::MomentumTier;
use springtale_cooperation::pacing::PacingManager;
use springtale_cooperation::types::{ApprovalPolicy, FormationConstraints};

use crate::cooperation::blackboard::trait_::Blackboard;
use crate::cooperation::formation::{Formation, FormationMember};
use crate::runtime::tick_steps::resolve_consensus;

use super::post::{PostEnv, post_member};
use super::test_support::{Runtime, earn_fever_and_peak, make_tick, runtime, wipe_task};
use super::{ExecuteCtx, Prepared, dispatch_one, prepare};

fn consensus_formation() -> Formation {
    let members: Vec<FormationMember> = (0..3)
        .map(|_| {
            FormationMember::from_strings(
                AgentId::new(),
                vec![super::test_support::CONNECTOR.into()],
            )
        })
        .collect();
    Formation::new_disconnected(
        members,
        IntentPattern::Execute { plan_id: None },
        FormationConstraints {
            destructive_action_policy: ApprovalPolicy::RequireConsensus,
            ..FormationConstraints::default()
        },
    )
}

/// One prepare → dispatch → post pass for an autonomous member under
/// `RequireConsensus`. Returns the consensus proposal, if one opened.
async fn run_executor(
    formation: &mut Formation,
    rt: &Runtime,
    pacing: &mut PacingManager,
    member: &mut FormationMember,
    task: &SubTask,
    momentum: MomentumTier,
) -> Option<SubTask> {
    let tick = make_tick(1, Duration::from_millis(33));
    let prepared = prepare(ExecuteCtx {
        formation_id: formation.id.0,
        formation_momentum: momentum,
        destructive_policy: ApprovalPolicy::RequireConsensus,
        blackboard: &formation.blackboard,
        fuel: formation.fuel.as_ref(),
        pacing,
        member,
        tick: &tick,
        chosen_task: Some(task.clone()),
        tick_action: None,
        autonomy: springtale_cooperation::AutonomyLevel::ActAutonomously,
        bridge: &rt.bridge,
        sentinel: &rt.sentinel,
        registry: &rt.registry,
        sacrifice: None,
        awaiting_consensus: &formation.awaiting_consensus,
        consensus_approved: &mut formation.consensus_approved,
        cooperation_tx: None,
    })
    .await;
    let mut outcome = match prepared {
        Prepared::Settled(outcome) => *outcome,
        Prepared::Dispatch(job) => dispatch_one(*job).await,
    };
    let proposal = outcome.consensus_task.take();
    let env = PostEnv {
        formation_id: formation.id.0,
        blackboard: formation.blackboard.as_ref(),
        fuel: formation.fuel.as_ref(),
        shared_env: formation.shared_env.as_ref(),
        surfaces: formation.surfaces.as_ref(),
        direct_inbox: formation.direct_inbox.as_ref(),
        cooperation_tx: None,
    };
    post_member(member, &env, outcome, &tick);
    proposal
}

#[tokio::test]
async fn consensus_round_trip_propose_guard_approve_execute_once() {
    let rt = runtime(false, Duration::ZERO);
    let mut formation = consensus_formation();
    let voters: Vec<AgentId> = formation.members.iter().map(|m| m.agent_id).collect();
    let mut pacing = earn_fever_and_peak(&mut formation);
    let task = wipe_task();
    let mut member = formation.members[0].clone();

    // ── Call #1: destructive task opens a proposal, doesn't execute ──
    let proposal = run_executor(
        &mut formation,
        &rt,
        &mut pacing,
        &mut member,
        &task,
        MomentumTier::Fever,
    )
    .await
    .expect("call #1 opens a proposal");
    assert_eq!(rt.probe.executions(), 0, "nothing executed yet");

    // agent_pipeline's gather block: propose + register the guard.
    let vote_id = formation.consensus.propose(
        DecisionDescriptor {
            description: "execute consensus-target::wipe".into(),
            options: vec!["approve".into(), "deny".into()],
            required_participants: voters.len() as u32,
            subject: DecisionSubject::DestructiveAction {
                task: proposal.clone(),
            },
        },
        Duration::from_secs(60),
        &voters,
        1,
    );
    formation.awaiting_consensus.insert(proposal.id, vote_id);

    // ── Call #2: pending-vote guard blocks a duplicate proposal ──
    let proposal = run_executor(
        &mut formation,
        &rt,
        &mut pacing,
        &mut member,
        &task,
        MomentumTier::Fever,
    )
    .await;
    assert!(
        proposal.is_none(),
        "guard prevents a second proposal for the same task"
    );
    assert_eq!(rt.probe.executions(), 0);

    // ── Approve: every member casts a ballot, resolution applies ──
    for voter in &voters {
        formation
            .consensus
            .vote(&vote_id, *voter, VoteChoice::Option(0))
            .unwrap();
    }
    resolve_consensus::run(&mut formation, None);
    assert!(
        formation.consensus_approved.contains(&task.id),
        "approval minted the one-shot permit"
    );
    assert!(!formation.awaiting_consensus.contains_key(&task.id));

    // ── Call #3: permit consumed, connector runs exactly once ──
    let proposal = run_executor(
        &mut formation,
        &rt,
        &mut pacing,
        &mut member,
        &task,
        MomentumTier::Fever,
    )
    .await;
    assert!(proposal.is_none(), "no re-vote after approval");
    assert_eq!(
        rt.probe.executions(),
        1,
        "the destructive action executed EXACTLY once"
    );
    assert!(
        formation.consensus_approved.is_empty(),
        "the permit was one-shot — consumed on execution"
    );
    assert!(
        formation.blackboard.read_result(task.id).is_some(),
        "result posted to the blackboard"
    );
    assert!(
        member.active_task.is_none(),
        "post cleared the member's slot"
    );
}

#[tokio::test]
async fn test_execute_cold_destructive_require_consensus_proposes_without_dispatch() {
    let rt = runtime(false, Duration::ZERO);
    let mut formation = consensus_formation();
    assert_eq!(formation.momentum.tier, MomentumTier::Cold);
    let task = wipe_task();
    let mut member = formation.members[0].clone();
    let other = formation.members[1].agent_id;
    let mut pacing = PacingManager::default();

    let proposal = run_executor(
        &mut formation,
        &rt,
        &mut pacing,
        &mut member,
        &task,
        MomentumTier::Cold,
    )
    .await
    .expect("Cold tier opens a vote — momentum never skips consensus");
    assert_eq!(proposal.id, task.id);
    assert_eq!(rt.probe.executions(), 0, "no dispatch without a vote");
    assert!(member.active_task.is_none(), "member dropped the task");
    assert!(
        formation
            .blackboard
            .claim_task(&task.id.to_string(), other, formation.fuel.as_ref())
            .is_ok(),
        "blackboard claim released — another member can claim it"
    );
}

#[tokio::test]
async fn test_execute_cold_read_only_require_consensus_dispatches_without_vote() {
    let rt = runtime(true, Duration::ZERO);
    let mut formation = consensus_formation();
    let task = wipe_task();
    let mut member = formation.members[0].clone();
    let mut pacing = PacingManager::default();

    let proposal = run_executor(
        &mut formation,
        &rt,
        &mut pacing,
        &mut member,
        &task,
        MomentumTier::Cold,
    )
    .await;
    assert!(
        proposal.is_none(),
        "read-only manifest hint: nothing to vote on"
    );
    assert_eq!(rt.probe.executions(), 1, "dispatched immediately");
}
