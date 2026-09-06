//! Phase 3 of the beat: cleared members act together inside
//! `tick.window`, at most `constraints.max_concurrent_actions` at once
//! (plan 1.14; `0` means unbounded). Whatever has not finished when the
//! window closes keeps running as `FormationMember::pending` and reports
//! `Requested` (plan 0.3) until a later beat collects it.

use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use tokio::sync::mpsc;
use tokio::task::{JoinError, JoinHandle};

use springtale_cooperation::action_state::ActionState;
use springtale_cooperation::cadence::{ActionDescriptor, AgentId, Tick};

use crate::cooperation::blackboard::trait_::Blackboard;
use crate::cooperation::dispatch_outcome::PendingDispatch;
use crate::cooperation::formation::Formation;

use super::super::executor::{DispatchJob, ExecuteOutcome, dispatch_one};

struct InFlight {
    handle: JoinHandle<ExecuteOutcome>,
    task_id: uuid::Uuid,
    descriptor: ActionDescriptor,
}

pub async fn run(
    formation: &mut Formation,
    jobs: Vec<DispatchJob>,
    tick: &Tick,
) -> Vec<(AgentId, ExecuteOutcome)> {
    let mut outcomes = Vec::new();

    // Earlier beats first: a finished carry-over is this beat's result;
    // an unfinished one reports `Requested` again.
    for member in &mut formation.members {
        let Some(pending) = member.pending.take() else {
            continue;
        };
        if pending.handle.is_finished() {
            let outcome = match pending.handle.await {
                Ok(outcome) => outcome,
                Err(err) => {
                    formation
                        .blackboard
                        .release_task(&pending.task_id.to_string());
                    member.active_task = None;
                    join_failed(pending.descriptor, &err)
                }
            };
            outcomes.push((member.agent_id, outcome));
        } else {
            outcomes.push((
                member.agent_id,
                ExecuteOutcome::requested(pending.descriptor.clone()),
            ));
            member.pending.set(pending);
        }
    }

    // This beat's dispatches, bounded by the cap and the window.
    let cap = formation.constraints.max_concurrent_actions;
    let (done_tx, mut done_rx) = mpsc::unbounded_channel::<AgentId>();
    let mut queue: VecDeque<DispatchJob> = jobs.into();
    let mut running: HashMap<AgentId, InFlight> = HashMap::new();
    let deadline = tokio::time::sleep(tick.window);
    tokio::pin!(deadline);
    loop {
        while (cap == 0 || running.len() < cap)
            && let Some(job) = queue.pop_front()
        {
            let agent = job.agent;
            let in_flight = InFlight {
                task_id: job.task.id,
                descriptor: job.descriptor.clone(),
                handle: {
                    let done_tx = done_tx.clone();
                    tokio::spawn(async move {
                        let outcome = dispatch_one(job).await;
                        let _ = done_tx.send(agent);
                        outcome
                    })
                },
            };
            running.insert(agent, in_flight);
        }
        if running.is_empty() {
            break;
        }
        tokio::select! {
            Some(agent) = done_rx.recv() => {
                if let Some(in_flight) = running.remove(&agent) {
                    let outcome = match in_flight.handle.await {
                        Ok(outcome) => outcome,
                        Err(err) => {
                            formation.blackboard.release_task(&in_flight.task_id.to_string());
                            if let Some(member) = formation.member_mut(&agent) {
                                member.active_task = None;
                            }
                            join_failed(in_flight.descriptor, &err)
                        }
                    };
                    outcomes.push((agent, outcome));
                }
            }
            _ = &mut deadline => break,
            else => break,
        }
    }

    // Anyone still working past the beat keeps working and reports on a
    // later beat (Left 4 Dead: a teammate mid-action is not the team's
    // problem until it is).
    for (agent, in_flight) in running {
        match formation.member_mut(&agent) {
            Some(member) => {
                outcomes.push((
                    agent,
                    ExecuteOutcome::requested(in_flight.descriptor.clone()),
                ));
                member.pending.set(PendingDispatch {
                    handle: in_flight.handle,
                    task_id: in_flight.task_id,
                    descriptor: in_flight.descriptor,
                    since: Instant::now(),
                    since_tick: tick.sequence,
                });
            }
            None => in_flight.handle.abort(),
        }
    }

    // The cap held these past the window: release the claim so the task
    // is scanned again next beat, and report the deferral like pacing.
    for job in queue {
        job.blackboard.release_task(&job.task.id.to_string());
        if let Some(member) = formation.member_mut(&job.agent) {
            member.active_task = None;
        }
        tracing::debug!(
            formation = %job.formation_id,
            agent = %job.agent.0,
            cap,
            "claim deferred — concurrent action cap held it past the beat"
        );
        outcomes.push((job.agent, ExecuteOutcome::settled(None, 0.7)));
    }

    outcomes
}

/// A spawned dispatch panicked or was cancelled: the connector never
/// answered, so the member reports a failure with no result row.
fn join_failed(descriptor: ActionDescriptor, err: &JoinError) -> ExecuteOutcome {
    tracing::warn!(error = %err, "member dispatch task did not complete");
    ExecuteOutcome {
        state: ActionState::Failure(err.to_string()),
        ..ExecuteOutcome::settled(Some(descriptor), 0.3)
    }
}
