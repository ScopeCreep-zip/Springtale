//! Step 4d — supervisor health checks (`COOPERATION.md §15.2` Erlang OTP).
//!
//! For each member the supervisor classifies the failure category and emits
//! a `SupervisionAction`. B10 wires real execution per the plan:
//!   * `MarkDown` → mutate `member.liveness = Liveness::Down` + broadcast
//!     `AgentDown` on the formation bus.
//!   * `Escalate` → set `formation.escalation_pending` for `check_interventions`
//!     (B1) to fold into `InterventionSignals` next tick.
//!   * `RetryWithRally` → run `cascade::attempt_self_rally` and persist the
//!     consumed token count.
//!   * `TriggerReplan` → set `formation.needs_replan` for the L5 CBBA
//!     executor (`replan_cbba::run`, B3) to consume next tick.
//!   * `TransformRole` → no-op here; `transformation::run` (step 10) owns
//!     the transformation pipeline and runs every tick anyway.

use tokio::sync::broadcast;

use crate::cooperation::formation::Formation;
use springtale_cooperation::comms::{BroadcastTrigger, StateBroadcastMsg, StateMessage};
use springtale_cooperation::events::{self, CooperationEvent, CooperationEventEnvelope};
use springtale_cooperation::rally::cascade;
use springtale_cooperation::supervision::{Liveness, SupervisionAction};
use springtale_cooperation::tick_processor::FormationTickResult;

pub fn run(
    formation: &mut Formation,
    result: &FormationTickResult,
    cooperation_tx: Option<&broadcast::Sender<CooperationEventEnvelope>>,
) {
    let cascade_count = result.interferences.len() as u32;

    // Collect actions first so the supervisor's `&formation.members`
    // borrow is released before mutating per-action.
    let mut actions: Vec<SupervisionAction> = Vec::new();
    for member in &formation.members {
        if let Some(action) = formation.supervisor.check_member(
            member.agent_id,
            member.liveness,
            member.consecutive_failures,
            cascade_count,
            &formation.rally.tokens,
        ) {
            actions.push(action);
        }
    }

    let formation_id = formation.id.0.to_string();
    for action in actions {
        execute(&formation_id, formation, action, cooperation_tx);
    }
}

fn execute(
    formation_id: &str,
    formation: &mut Formation,
    action: SupervisionAction,
    cooperation_tx: Option<&broadcast::Sender<CooperationEventEnvelope>>,
) {
    match action {
        SupervisionAction::TransformRole { agent } => {
            // Owned by `transformation::run` (step 10) which evaluates
            // every member's role every tick. Logging only here so the
            // supervisor decision is observable.
            tracing::info!(
                formation = formation_id,
                agent = %agent.0,
                "supervisor: transform role (handled by step 10)"
            );
        }
        SupervisionAction::RetryWithRally { agent } => {
            let result = cascade::attempt_self_rally(
                &formation.rally,
                &formation.attention_broker,
                &mut formation.momentum,
                agent,
            );
            tracing::info!(
                formation = formation_id,
                agent = %agent.0,
                ?result,
                "supervisor: retry with rally"
            );
        }
        SupervisionAction::TriggerReplan => {
            // Flag picked up by `replan_cbba::run` next tick (B3).
            formation.needs_replan = true;
            tracing::warn!(
                formation = formation_id,
                "supervisor: trigger replan — needs_replan flag set"
            );
        }
        SupervisionAction::MarkDown { agent, since_tick } => {
            if let Some(member) = formation.member_mut(&agent) {
                member.liveness = Liveness::Down { since_tick };
            }
            formation.bus.broadcast_state(StateBroadcastMsg {
                source: agent,
                trigger: BroadcastTrigger::AgentDown(agent),
                message: StateMessage {
                    content: format!("marked down by supervisor at tick {since_tick}"),
                    severity: 0.95,
                },
            });
            tracing::warn!(
                formation = formation_id,
                agent = %agent.0,
                since_tick = since_tick.0,
                "supervisor: marked down + AgentDown broadcast"
            );
            // Phase H5: high-severity event surfaces in the EventRibbon.
            events::emit(
                cooperation_tx,
                CooperationEvent::MemberMarkedDown {
                    formation_id: formation.id,
                    agent,
                    since_tick,
                },
            );
        }
        SupervisionAction::Escalate { reason } => {
            // Flag picked up by `check_interventions::run` next tick (B1)
            // and folded into the L6 intervention signals.
            formation.escalation_pending = Some(reason.clone());
            tracing::error!(
                formation = formation_id,
                reason = %reason,
                "supervisor: escalation flag set for L6 intervention"
            );
            // Phase H5: surface escalation immediately so the user sees
            // the next-tick L6 intervention coming.
            events::emit(
                cooperation_tx,
                CooperationEvent::SupervisorEscalated {
                    formation_id: formation.id,
                    reason: reason.clone(),
                },
            );
        }
    }
}
