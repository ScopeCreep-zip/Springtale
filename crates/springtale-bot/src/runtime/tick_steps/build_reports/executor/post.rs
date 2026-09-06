//! Gather-phase half of the executor: one member writes its own result
//! back (peer writes, as before). Nothing here arbitrates between
//! members — `tick_processor` reads the beat's write log for that.

use std::time::Duration;

use tokio::sync::broadcast;

use springtale_cooperation::CooperationEventEnvelope;
use springtale_cooperation::cadence::{AgentId, Tick, TickReport};
use springtale_cooperation::stigmergy::types::SurfaceType;

use crate::cooperation::blackboard::cooperative::CooperativeBlackboard;
use crate::cooperation::blackboard::trait_::Blackboard;
use crate::cooperation::formation::{Formation, FormationMember};
use crate::orchestrator::fuel::FuelBudget;

use super::ExecuteOutcome;

/// Formation handles `post_member` writes through — disjoint field
/// borrows so the member itself can be `&mut` at the same time.
pub struct PostEnv<'a> {
    pub formation_id: uuid::Uuid,
    pub blackboard: &'a CooperativeBlackboard,
    pub fuel: &'a FuelBudget,
    pub shared_env: &'a springtale_cooperation::state::SharedEnvironment,
    pub surfaces: &'a dyn springtale_cooperation::stigmergy::SurfaceSubstrate,
    pub direct_inbox: &'a springtale_cooperation::routing::direct::DirectInbox,
    pub cooperation_tx: Option<&'a broadcast::Sender<CooperationEventEnvelope>>,
}

/// Post one member's outcome and sample its attention load. `None` when
/// the member left the formation while its dispatch was in flight.
pub fn post(
    formation: &mut Formation,
    agent: AgentId,
    outcome: ExecuteOutcome,
    tick: &Tick,
    cooperation_tx: Option<&broadcast::Sender<CooperationEventEnvelope>>,
) -> Option<TickReport> {
    let env = PostEnv {
        formation_id: formation.id.0,
        blackboard: formation.blackboard.as_ref(),
        fuel: formation.fuel.as_ref(),
        shared_env: formation.shared_env.as_ref(),
        surfaces: formation.surfaces.as_ref(),
        direct_inbox: formation.direct_inbox.as_ref(),
        cooperation_tx,
    };
    let duration_ms = outcome.duration_ms;
    let member = formation.members.iter_mut().find(|m| m.agent_id == agent)?;
    let report = post_member(member, &env, outcome, tick);

    // Attention is earned by acting (Army of Two aggro): a member with
    // work in hand — or still in flight — generates load this beat, and
    // a long connector call generates more; idle members drain.
    let busy = member.active_task.is_some() || member.pending.is_some();
    let sample = if busy { 0.5 } else { 0.0 } + (duration_ms as f32 / 1000.0).min(0.3);
    formation.attention_broker.observe(agent, sample, 0.3);
    Some(report)
}

/// The member's own write-back: active-task state, result row, W3 push
/// handoff, §13 audit write, L0 stigmergy deposit, and its `TickReport`.
pub fn post_member(
    member: &mut FormationMember,
    env: &PostEnv<'_>,
    outcome: ExecuteOutcome,
    tick: &Tick,
) -> TickReport {
    if let Some(done) = outcome.dispatched {
        if let Some(active) = member.active_task.as_mut() {
            active.begin_execution();
            if done.success {
                active.succeed();
            } else {
                active.fail(done.error.clone().unwrap_or_default());
            }
        }

        let sub_result = springtale_cooperation::SubTaskResult {
            task_id: done.task.id,
            agent_id: member.agent_id,
            success: done.success,
            output: done.output,
            duration_ms: outcome.duration_ms,
        };
        let _ = env.blackboard.post_result(&sub_result, env.fuel);

        // W3 push handoff: this result may have unblocked dependents. Any
        // now-claimable task that (a) depended on this one and (b) carries
        // an `assigned_to` hint is pushed to that agent's inbox so the L3
        // inbox step picks it up next beat, preempting the scan (§20.1).
        if done.success {
            for dep in env.blackboard.scan_tasks(&[]) {
                if dep.depends_on.contains(&done.task.id)
                    && let Some(target) = dep.assigned_to
                {
                    springtale_cooperation::routing::direct::assignment::assign(
                        env.direct_inbox,
                        target,
                        dep,
                    );
                }
            }
        }

        // §13 audit-log entry — feeds the tick processor's
        // `detect_from_records_with_history` so ActionNegation has real
        // history. `shared_env.write` (non-CAS) because this is an ordered
        // audit log, not a conflict probe.
        if let (
            true,
            springtale_core::rule::action::Action::RunConnector {
                connector,
                action: action_name,
                ..
            },
        ) = (done.success, &done.action)
        {
            let key = format!(
                "action:{}:{}:{}:{}",
                tick.sequence, member.agent_id.0, connector, action_name
            );
            let record = serde_json::json!({
                "tick": tick.sequence,
                "agent": member.agent_id.0.to_string(),
                "duration_ms": outcome.duration_ms,
            });
            env.shared_env.write(&key, record, member.agent_id);

            // B4 — L0 stigmergy: deposit a `Substrate` surface tagged with
            // the connector capability so peers sense recent activity
            // (`COOPERATION.md §10`). 60s TTL — surfaces fade before stale
            // data misleads scan_and_claim.
            env.surfaces.deposit(
                member.agent_id,
                SurfaceType::Substrate,
                serde_json::json!({
                    "connector": connector,
                    "action": action_name,
                    "tick": tick.sequence,
                }),
                Some(Duration::from_secs(60)),
                Some(springtale_cooperation::capability::CapabilityDecl::new(
                    connector,
                )),
            );
            springtale_cooperation::events::emit(
                env.cooperation_tx,
                springtale_cooperation::events::CooperationEvent::SurfaceDeposited {
                    formation_id: springtale_cooperation::types::FormationId(env.formation_id),
                    agent: member.agent_id,
                    surface_kind: format!("substrate:{connector}:{action_name}"),
                    ttl_ms: 60_000,
                },
            );
        }

        member.active_task = None;
    }

    TickReport {
        agent_id: member.agent_id,
        tick_sequence: tick.sequence,
        action_taken: outcome.action_descriptor,
        latency: Duration::from_millis(outcome.duration_ms),
        intent_alignment: outcome.alignment,
        interference_with: vec![],
    }
}
