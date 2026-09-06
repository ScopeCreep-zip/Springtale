//! The beat (plan 1.8): every operational member decides against the
//! same frozen snapshot, all decided members act together inside
//! `tick.window`, and the results are gathered in agent-id order so two
//! runs with the same inputs produce the same log.
//!
//! COOPERATION.pdf §2.3 (Crypt of the NecroDancer): "The beat belongs to
//! the music, not to any player... Every beat is a simultaneous decision
//! point." §2.4 (Splinter Cell): "Both must execute simultaneously."
//! §13 treats conflicts as detected after the fact from surfaces and the
//! write log, not prevented by a lock — so nothing here arbitrates;
//! `tick_processor` runs over this beat's write log in `build_reports`.
//!
//! Phases:
//! 1. **decide** (`decide.rs`) — sense, inbox, react, scan, respond_cfp,
//!    sacrifice per member, against one snapshot (plan 1.9 order).
//! 2. **claim** — `executor::prepare` claims on the blackboard now and
//!    runs the autonomy / pacing / consensus gates.
//! 3. **act** (`act.rs`) — one spawned `dispatch_one` per cleared member,
//!    at most `constraints.max_concurrent_actions` in flight (1.14),
//!    collected until the window closes; the rest carry over as
//!    `FormationMember::pending` and report `Requested` (0.3).
//! 4. **gather** — sort by agent id, `executor::post` each, then open the
//!    consensus votes the beat proposed (`consensus.rs`).

mod act;
mod consensus;
mod decide;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use tokio::sync::mpsc;

use springtale_cooperation::cadence::{AgentId, Tick, TickReport};

use crate::cooperation::formation::Formation;

use super::executor::{self, ExecuteCtx, ExecuteOutcome, Prepared};
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
    // Top of the beat: the CFPs that arrived since the last one.
    formation.drain_open_cfps();
    let snapshots = decide::Snapshots::capture(formation);
    let drained_state = state_drain::run(formation);

    // Members have no rule of their own yet (synthesized formation rules
    // are owned by the formation), so autonomy resolves at the formation
    // level: `autonomy:formation:{id}`, else ActAutonomously.
    let autonomy = springtale_runtime::operations::agent::resolve_formation_autonomy(
        store.as_ref(),
        &formation.id.0.to_string(),
    )
    .await;

    // 1. Decide. A member whose dispatch is still in flight is busy — it
    // is collected or reported `Requested` by `act`, not re-decided.
    let mut decisions = Vec::new();
    for member in formation
        .members
        .iter_mut()
        .filter(|m| m.is_operational() && !m.pending.is_some())
    {
        let drained = drained_state.get(&member.agent_id);
        decisions.push(decide::run(member, tick, &snapshots, drained).await);
    }
    for decision in &mut decisions {
        if let Some(bid) = decision.bid.take()
            && formation.cfp_channels.bid_tx.send(bid).is_err()
        {
            tracing::trace!(formation = %formation.id.0, "bid send failed (initiator gone)");
        }
    }

    // 2. Claim on the blackboard now.
    let mut settled: Vec<(AgentId, ExecuteOutcome)> = Vec::new();
    let mut jobs = Vec::new();
    for decision in decisions {
        let Some(member) = formation
            .members
            .iter_mut()
            .find(|m| m.agent_id == decision.agent)
        else {
            continue;
        };
        let prepared = executor::prepare(ExecuteCtx {
            formation_id: formation.id.0,
            formation_momentum: snapshots.momentum.tier,
            destructive_policy: formation.constraints.destructive_action_policy,
            blackboard: &formation.blackboard,
            fuel: formation.fuel.as_ref(),
            pacing: &mut formation.pacing,
            member,
            tick,
            chosen_task: decision.chosen_task,
            tick_action: decision.tick_action,
            autonomy,
            bridge,
            sentinel,
            registry,
            sacrifice: decision.sacrifice,
            awaiting_consensus: &formation.awaiting_consensus,
            consensus_approved: &mut formation.consensus_approved,
            cooperation_tx,
        })
        .await;
        match prepared {
            Prepared::Settled(outcome) => settled.push((decision.agent, *outcome)),
            Prepared::Dispatch(job) => jobs.push(*job),
        }
    }

    // 3. Act together, bounded by the beat.
    let mut outcomes = act::run(formation, jobs, tick).await;
    outcomes.extend(settled);

    // 4. Gather in agent-id order; each member posts its own results.
    outcomes.sort_by_key(|(agent, _)| agent.0);
    let mut proposals = Vec::new();
    let mut reports = Vec::new();
    for (agent, mut outcome) in outcomes {
        if let Some(task) = outcome.consensus_task.take() {
            proposals.push(task);
        }
        if let Some(report) = executor::post(formation, agent, outcome, tick, cooperation_tx) {
            let _ = reports_sender.try_send(report.clone());
            reports.push(report);
        }
    }
    consensus::propose_all(formation, proposals, cooperation_tx);

    reports
}
