//! Per-member agent pipeline — runs one `AgentLoop::tick`-equivalent
//! composition per operational member, then dispatches the result through
//! `executor::execute`.
//!
//! Plan §A2 layer order: L0 sense → L3 inbox → L2 react → L1 scan. Each
//! step is the trait-bounded function in
//! `springtale-cooperation::agent::step::*`. An inbox hit early-exits
//! (the handoff is the tick's task); a surface reaction does not — the
//! scan still runs so a primed surface never starves task pickup
//! (plan 1.9 / finding 40). React folds bus state messages into
//! awareness without producing a tick action.
//!
//! - **L0 sense (B4):** primed-surface reaction via `SurfaceSensor`.
//!   Returns Some without `task_claimed` when a surface fires; the tick
//!   reports the surface_reaction action unless L1 scan claims a task,
//!   in which case the report carries the task.
//! - **L3 inbox (B6):** direct-handoff via `TaskRouter::poll_assigned`.
//!   Narrows to one assigned SubTask.
//! - **L2 react:** drains pre-collected bus state messages via
//!   `BufferedStateSubscriber` into the member's awareness so the
//!   subsequent scan sees fresh peer-state. No tick action.
//! - **L1 scan (B5):** `TaskRouter::scan` with capability + tier filter.
//!
//! EnvironmentMediated handoff flows through L0 sense (stigmergy).
//! FlexibleChain work-stealing lives in the runner task spawned by
//! `Formation::start_member_runners`.
//!
//! After the pipeline picks a task (or doesn't), `executor::execute`
//! handles claim → pacing gate → consensus gate → dispatch → audit log
//! → stigmergy deposit, gated by autonomy level. Consensus-gated tasks
//! are collected during the loop and fired against `formation.consensus`
//! after the borrow on `formation.members` releases.

use std::sync::Arc;

use tokio::sync::mpsc;

use springtale_cooperation::agent::AgentContext;
use springtale_cooperation::agent::step;
use springtale_cooperation::cadence::{Tick, TickReport};
use springtale_cooperation::dissemination::BufferedStateSubscriber;

use crate::cooperation::formation::Formation;

use super::executor;
use super::state_drain;

pub async fn run(
    formation: &mut Formation,
    tick: &Tick,
    bridge: &springtale_runtime::CapabilityBridge,
    sentinel: &Arc<springtale_sentinel::Sentinel>,
    registry: &Arc<tokio::sync::RwLock<springtale_connector::registry::store::ConnectorRegistry>>,
    store: &Arc<dyn springtale_store::StorageBackend>,
    reports_sender: &mpsc::Sender<TickReport>,
    cooperation_tx: Option<
        &tokio::sync::broadcast::Sender<springtale_cooperation::CooperationEventEnvelope>,
    >,
) -> Vec<TickReport> {
    let mut reports = Vec::new();
    let formation_momentum = formation.momentum.tier;
    let mut consensus_proposals: Vec<springtale_cooperation::action::SubTask> = Vec::new();

    // Per-tick formation snapshot so per-member `AgentContext` construction
    // inside the loop doesn't fight the `&mut formation.members` borrow.
    let fc_snapshot = springtale_cooperation::context::FormationContext {
        intent: formation.intent.clone(),
        momentum_tier: formation.momentum.tier,
        constraints: formation.constraints.clone(),
        guard_mode: formation.constraints.guard_mode,
        operational_count: formation.operational_count(),
        member_count: formation.members.len(),
        paused: formation.paused,
    };
    let momentum_snapshot = formation.momentum.clone();
    let attention_snapshot = formation.attention_broker.current();
    let task_router = formation.task_router.clone();
    let surfaces = formation.surfaces.clone();
    let member_count_snapshot = formation.members.len();
    let rally_tokens_snapshot = formation.rally.tokens.remaining() as u32;

    let drained_state = state_drain::run(formation);

    for member in &mut formation.members {
        if !member.is_operational() {
            continue;
        }

        // Members have no rule of their own yet (synthesized formation rules
        // are owned by the formation), so autonomy resolves at the formation
        // level: `autonomy:formation:{id}`, else ActAutonomously.
        let autonomy = springtale_runtime::operations::agent::resolve_formation_autonomy(
            store.as_ref(),
            &formation.id.0.to_string(),
        )
        .await;

        // Plan §A2 layer order: L0 sense → L3 inbox → L2 react → L1 scan.
        // Borrow scoping: react needs `&mut member.awareness`, while
        // sense + inbox + scan all need `&member.awareness` via
        // `AgentContext`. We construct the ctx in two phases (pre-react
        // for sense+inbox, post-react for scan) so the immutable borrow
        // is released before react mutates.
        let mut tick_action: Option<springtale_cooperation::cadence::ActionDescriptor> = None;
        let mut chosen_task: Option<springtale_cooperation::action::SubTask> = None;
        let mut needs_scan = true;

        // Phase 1: sense + inbox (read-only awareness).
        {
            let agent_ctx = AgentContext {
                agent_id: member.agent_id,
                tick,
                formation: &fc_snapshot,
                momentum: &momentum_snapshot,
                attention: &attention_snapshot,
                capabilities: &member.capabilities,
                awareness: &member.awareness,
            };
            if let Some(r) = step::sense::run(surfaces.as_ref(), &member.awareness, &agent_ctx) {
                // A surface reaction is not a task claim: it must not
                // starve task pickup, so the scan still runs below
                // (plan 1.9 / finding 40). Only an inbox hit skips it.
                tick_action = r.action;
                chosen_task = r.task_claimed; // None for surface_reaction
            } else if let Some(r) = step::inbox::run(task_router.as_ref(), &agent_ctx).await {
                tick_action = r.action;
                chosen_task = r.task_claimed;
                needs_scan = false;
            }
        }

        let mut sacrifice_action: Option<springtale_cooperation::sacrifice::SacrificeAction> = None;
        if needs_scan {
            // Phase 2: react (mutates awareness) + scan (re-reads it) +
            // sacrifice (peer-aware "final consideration" per plan §B9).
            if let Some(msgs) = drained_state.get(&member.agent_id) {
                let mut buf = BufferedStateSubscriber::new(msgs.clone());
                step::react::run(&mut buf, &mut member.awareness, momentum_snapshot.tier);
            }
            let agent_ctx = AgentContext {
                agent_id: member.agent_id,
                tick,
                formation: &fc_snapshot,
                momentum: &momentum_snapshot,
                attention: &attention_snapshot,
                capabilities: &member.capabilities,
                awareness: &member.awareness,
            };
            if let Some(r) = step::scan::run(task_router.as_ref(), &agent_ctx).await {
                tick_action = r.action;
                chosen_task = r.task_claimed;
            }
            // B9 final consideration — at Hot+ tier, the agent voluntarily
            // checks if yielding to a more-loaded peer is the higher-utility
            // play. Returns `Some(SacrificeAction)` to override the scan
            // pick; the executor drops `chosen_task` and emits a yield-shaped
            // tick report.
            sacrifice_action = step::sacrifice::run(
                &agent_ctx,
                rally_tokens_snapshot,
                member_count_snapshot,
                &[],
            );
        }
        if sacrifice_action.is_some() {
            chosen_task = None;
        }

        let exec_outcome = executor::execute(executor::ExecuteCtx {
            formation_id: formation.id.0,
            formation_momentum,
            destructive_policy: formation.constraints.destructive_action_policy,
            registry,
            blackboard: formation.blackboard.as_ref(),
            shared_env: formation.shared_env.as_ref(),
            surfaces: formation.surfaces.as_ref(),
            fuel: formation.fuel.as_ref(),
            pacing: &mut formation.pacing,
            member,
            tick,
            chosen_task,
            tick_action,
            autonomy,
            bridge,
            sentinel,
            direct_inbox: formation.direct_inbox.as_ref(),
            sacrifice: sacrifice_action,
            awaiting_consensus: &formation.awaiting_consensus,
            consensus_approved: &mut formation.consensus_approved,
            cooperation_tx,
        })
        .await;

        if let Some(task) = exec_outcome.consensus_task {
            consensus_proposals.push(task);
        }

        // Attention is earned by acting (Army of Two aggro): a member with
        // work in hand generates load this tick; idle members drain. Only
        // the active-task term exists today — `ExecuteOutcome` carries no
        // `duration_ms` and `FormationMember` has no `pending` slot.
        let sample = if member.active_task.is_some() {
            0.5
        } else {
            0.0
        };
        formation
            .attention_broker
            .observe(member.agent_id, sample, 0.3);

        let report = TickReport {
            agent_id: member.agent_id,
            tick_sequence: tick.sequence,
            action_taken: exec_outcome.action_descriptor,
            latency: std::time::Duration::from_millis(0),
            intent_alignment: exec_outcome.alignment,
            interference_with: vec![],
        };

        let _ = reports_sender.try_send(report.clone());
        reports.push(report);
    }

    // B7 — fire collected consensus proposals after the per-member loop
    // releases its `&mut formation.members` borrow. The voters list is
    // every operational member; override tokens default to 1 per agent
    // (As Dusk Falls game default for high-stakes votes per spec §11).
    if !consensus_proposals.is_empty() {
        use springtale_cooperation::consensus::DecisionDescriptor;
        let voters: Vec<springtale_cooperation::cadence::AgentId> = formation
            .members
            .iter()
            .filter(|m| m.is_operational())
            .map(|m| m.agent_id)
            .collect();
        let voter_count = voters.len() as u32;
        for task in consensus_proposals {
            let task_id = task.id;
            let id = formation.consensus.propose(
                DecisionDescriptor {
                    description: format!(
                        "execute {}::{} (id={})",
                        task.target_connector.name, task.action_name, task.id
                    ),
                    options: vec!["approve".into(), "deny".into()],
                    required_participants: voter_count,
                    subject:
                        springtale_cooperation::consensus::DecisionSubject::DestructiveAction {
                            task,
                        },
                },
                std::time::Duration::from_secs(5),
                &voters,
                1,
            );
            // B7 guard — while this entry exists, the executor won't
            // re-propose for the same task on subsequent ticks.
            formation.awaiting_consensus.insert(task_id, id);
            tracing::info!(
                formation = %formation.id.0,
                vote_id = %id,
                task = %task_id,
                voters = voter_count,
                "consensus vote opened for destructive action"
            );
            // Phase H5: surface vote-opened so the formation event log
            // shows pending votes alongside the rest of the cooperation
            // lifecycle.
            springtale_cooperation::events::emit(
                cooperation_tx,
                springtale_cooperation::events::CooperationEvent::ConsensusVoteOpened {
                    formation_id: formation.id,
                    vote_id: id,
                    deadline_ms: 5_000,
                },
            );
        }
    }

    reports
}
