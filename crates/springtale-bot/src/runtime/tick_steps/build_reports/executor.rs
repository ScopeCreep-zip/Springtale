//! Per-agent task executor — claim → pacing gate → consensus gate →
//! dispatch → audit log → stigmergy deposit, gated by autonomy.
//!
//! This is the bot-side execution path that consumes the `chosen_task`
//! produced by the `agent::step::*` pipeline. Autonomy gating maps the
//! cooperation crate's `AutonomyLevel` (Observe/Suggest/Approve/
//! Autonomous) to AoE-style stance behavior:
//!
//! - **Observe** (AoE "No Attack"): no action even when a task was
//!   surfaced. Tick reports the `tick_action` descriptor only.
//! - **Suggest** (AoE "Stand Ground"): log the task without claiming.
//! - **Approve** (AoE "Defensive"): claim the task and mark `Requested`,
//!   wait for human approval (the approval flow flips it to `Executing`
//!   on a later tick — out of scope for this module).
//! - **Autonomous** (AoE "Aggressive"): claim and execute inline this
//!   tick.
//!
//! Active-task continuation: when `member.active_task` is already in
//! `Executing` or `Requested` from a prior tick, the executor returns
//! a continuation outcome that surfaces the in-flight descriptor on
//! the report and skips the new-claim path.

use std::sync::Arc;

use springtale_cooperation::AutonomyLevel;
use springtale_cooperation::MomentumTier;
use springtale_cooperation::action_state::ActiveTask;
use springtale_cooperation::cadence::{ActionDescriptor, Tick};
use springtale_cooperation::sacrifice::SacrificeAction;
use springtale_cooperation::stigmergy::types::SurfaceType;
use springtale_cooperation::types::ApprovalPolicy;

use crate::cooperation::blackboard::cooperative::CooperativeBlackboard;
use crate::cooperation::blackboard::trait_::Blackboard;
use crate::cooperation::formation::FormationMember;
use crate::orchestrator::fuel::FuelBudget;

pub struct ExecuteCtx<'a> {
    pub formation_id: uuid::Uuid,
    pub formation_momentum: MomentumTier,
    pub destructive_policy: ApprovalPolicy,
    pub blackboard: &'a CooperativeBlackboard,
    pub shared_env: &'a springtale_cooperation::state::SharedEnvironment,
    pub surfaces: &'a dyn springtale_cooperation::stigmergy::SurfaceSubstrate,
    pub fuel: &'a FuelBudget,
    pub pacing: &'a mut springtale_cooperation::pacing::PacingManager,
    pub member: &'a mut FormationMember,
    pub tick: &'a Tick,
    pub chosen_task: Option<springtale_cooperation::action::SubTask>,
    pub tick_action: Option<ActionDescriptor>,
    pub autonomy: AutonomyLevel,
    pub bridge: &'a springtale_runtime::CapabilityBridge,
    pub sentinel: &'a Arc<springtale_sentinel::Sentinel>,
    /// W3 push handoff (§20.1): when this member's result unblocks a
    /// dependent task that carries an `assigned_to` hint, the task is
    /// pushed straight to that agent's inbox (the inbox step preempts
    /// scan) instead of waiting for the next routine scan.
    pub direct_inbox: &'a springtale_cooperation::routing::direct::DirectInbox,
    /// B9: per-agent sacrifice action returned by `agent::step::sacrifice`.
    /// When `Some`, the executor short-circuits the claim/dispatch path
    /// and applies the sacrifice instead — for `Yield`, that means
    /// emitting a "yield" tick descriptor without claiming or dispatching.
    pub sacrifice: Option<SacrificeAction>,
    /// B7 guard: task ids with an open consensus vote. A task in this set
    /// is skipped (no claim, no second proposal) until the vote resolves.
    pub awaiting_consensus: &'a std::collections::HashMap<uuid::Uuid, uuid::Uuid>,
    /// B7 permits: one-shot execution approvals minted by an approving
    /// vote resolution. `remove` on claim — an approval authorizes
    /// exactly one execution.
    pub consensus_approved: &'a mut std::collections::HashSet<uuid::Uuid>,
    /// Phase H5: cooperation events broadcast sender. Optional so headless
    /// / test paths short-circuit. Used to emit `SacrificeYield`,
    /// `SurfaceDeposited`, and `ConsensusVoteOpened` envelopes from inside
    /// the executor where the relevant context lives.
    pub cooperation_tx: Option<
        &'a tokio::sync::broadcast::Sender<springtale_cooperation::CooperationEventEnvelope>,
    >,
}

pub struct ExecuteOutcome {
    pub action_descriptor: Option<ActionDescriptor>,
    pub alignment: f32,
    /// Set to `Some(task)` when a destructive action requires consensus
    /// at Fever tier — the agent_pipeline collects these and fires
    /// `consensus.propose` after the per-member loop releases its
    /// `&mut formation.members` borrow.
    pub consensus_task: Option<springtale_cooperation::action::SubTask>,
}

pub async fn execute(ctx: ExecuteCtx<'_>) -> ExecuteOutcome {
    // B9 short-circuit: if the agent voluntarily yielded this tick, emit a
    // yield-shaped tick descriptor and skip claim/dispatch. The chosen_task
    // (if any) was already cleared by the agent_pipeline when the
    // sacrifice fired. Logged at info so operators can see voluntary
    // sacrifices without a separate audit table.
    if let Some(SacrificeAction::Yield {
        sacrificer,
        beneficiary,
        utility,
    }) = ctx.sacrifice.clone()
    {
        tracing::info!(
            formation = %ctx.formation_id,
            sacrificer = %sacrificer.0,
            beneficiary = %beneficiary.0,
            utility,
            "agent voluntarily yielded this tick"
        );
        springtale_cooperation::events::emit(
            ctx.cooperation_tx,
            springtale_cooperation::events::CooperationEvent::SacrificeYield {
                formation_id: springtale_cooperation::types::FormationId(ctx.formation_id),
                sacrificer,
                beneficiary,
                utility,
            },
        );
        return ExecuteOutcome {
            action_descriptor: Some(ActionDescriptor {
                kind: "sacrifice_yield".to_owned(),
                target: Some(beneficiary.0.to_string()),
                payload_hash: 0,
            }),
            alignment: 0.9,
            consensus_task: None,
        };
    }

    // Continuation path: active_task carried over from a prior tick.
    if let Some(out) = continue_active_task(&ctx) {
        return out;
    }

    // No new task this tick — surface the step-side descriptor only.
    let Some(task) = ctx.chosen_task.clone() else {
        return ExecuteOutcome {
            action_descriptor: ctx.tick_action,
            alignment: 1.0,
            consensus_task: None,
        };
    };

    // Observe: no claim, no execute. Surface the step-side action.
    if ctx.autonomy == AutonomyLevel::Observe {
        return ExecuteOutcome {
            action_descriptor: ctx.tick_action,
            alignment: 1.0,
            consensus_task: None,
        };
    }

    // Suggest: log + report but don't claim.
    if ctx.autonomy == AutonomyLevel::Suggest {
        tracing::debug!(
            agent = %ctx.member.agent_id.0,
            task = %task.description,
            "agent suggests task (not claiming)"
        );
        return ExecuteOutcome {
            action_descriptor: ctx.tick_action,
            alignment: 1.0,
            consensus_task: None,
        };
    }

    // Approve / Autonomous: pacing gate first.
    if !ctx.pacing.allow_action() {
        tracing::debug!(
            formation = %ctx.formation_id,
            agent = %ctx.member.agent_id.0,
            phase = ctx.pacing.phase_name(),
            "claim deferred — pacing rate limit hit"
        );
        return ExecuteOutcome {
            action_descriptor: None,
            alignment: 0.7,
            consensus_task: None,
        };
    }

    // Claim on the blackboard.
    if ctx
        .blackboard
        .claim_task(&task.id.to_string(), ctx.member.agent_id, ctx.fuel)
        .is_err()
    {
        return ExecuteOutcome {
            action_descriptor: None,
            alignment: 0.5,
            consensus_task: None,
        };
    }

    let descriptor = crate::cooperation::task_dispatch::subtask_to_descriptor(&task);
    ctx.member.active_task = Some(ActiveTask::new(
        task.clone(),
        ctx.member.agent_id,
        ctx.tick.sequence,
    ));

    // B7 — destructive actions classified RequireConsensus must be voted
    // on at Fever tier instead of executing. Lower tiers fall through to
    // autonomy-based execution.
    let auto_execute = ctx.autonomy == AutonomyLevel::ActAutonomously;
    let needs_consensus = matches!(ctx.destructive_policy, ApprovalPolicy::RequireConsensus)
        && springtale_cooperation::authority::allows(
            ctx.formation_momentum,
            springtale_cooperation::layer::LayerId::L4Contested,
        );
    if auto_execute && needs_consensus && !ctx.consensus_approved.remove(&task.id) {
        // No one-shot approval permit for this task. Release the claim so
        // the task stays available, then either wait on the open vote or
        // open one.
        ctx.blackboard.release_task(&task.id.to_string());
        ctx.member.active_task = None;
        if ctx.awaiting_consensus.contains_key(&task.id) {
            // Vote already open — guard against re-proposing every tick.
            return ExecuteOutcome {
                action_descriptor: None,
                alignment: 0.8,
                consensus_task: None,
            };
        }
        return ExecuteOutcome {
            action_descriptor: Some(descriptor),
            alignment: 0.8,
            consensus_task: Some(task),
        };
    }

    // Approve mode: claim but stay in Requested for later approval.
    if !auto_execute {
        if let Some(active) = ctx.member.active_task.as_mut() {
            active.request();
        }
        return ExecuteOutcome {
            action_descriptor: None,
            alignment: 0.8,
            consensus_task: None,
        };
    }

    // Autonomous: dispatch immediately.
    if let Some(active) = ctx.member.active_task.as_mut() {
        active.request();
        active.begin_execution();
    }

    // W3 cross-agent data pipe: materialize upstream results into this
    // task's params (`${result:<uuid>...}`) before building the action.
    // The scan-side dependency gate guarantees the results exist.
    let mut task = task;
    crate::cooperation::task_dispatch::resolve_result_params(&mut task, ctx.blackboard);

    let action = crate::cooperation::task_dispatch::subtask_to_action(&task);
    let exec_start = std::time::Instant::now();

    // Formation-tick path: build the cooperation envelope with the
    // firing agent + formation + the formation's current momentum
    // tier. The dispatcher forwards `momentum` to `bridge.execute(...)`
    // so the per-tier WASM `InstancePre` selection matches the call
    // site (§16). The synthetic RuleId is fine — formation-task
    // dispatch is rule-less; the executions log (Phase B) keys off
    // `bot_id` + `formation_id` instead.
    let execution = springtale_cooperation::execution::ExecutionContext::for_formation(
        springtale_core::rule::RuleId::new(),
        ctx.member.agent_id,
        springtale_cooperation::types::FormationId(ctx.formation_id),
        ctx.formation_momentum,
        springtale_cooperation::execution::ExecutionMode::Cooperation,
    );
    let exec_result = springtale_runtime::dispatch::dispatch_action(
        &action,
        ctx.bridge,
        ctx.sentinel,
        execution,
        serde_json::Value::Null,
    )
    .await;
    let duration_ms = exec_start.elapsed().as_millis() as u64;

    let (success, output) = match &exec_result {
        Ok(chain) => (
            true,
            chain
                .steps
                .last()
                .map(|s| s.output.clone())
                .unwrap_or_else(|| serde_json::json!({ "result": chain.brief() })),
        ),
        Err(err) => (false, serde_json::json!({"error": err.to_string()})),
    };
    let error_msg = exec_result.err().map(|e| e.to_string());

    if let Some(active) = ctx.member.active_task.as_mut() {
        if success {
            active.succeed();
        } else {
            active.fail(error_msg.unwrap_or_default());
        }
    }

    let sub_result = springtale_cooperation::SubTaskResult {
        task_id: task.id,
        agent_id: ctx.member.agent_id,
        success,
        output,
        duration_ms,
    };
    let _ = ctx.blackboard.post_result(&sub_result, ctx.fuel);

    // W3 push handoff: this result may have unblocked dependents. Any
    // now-claimable task that (a) depended on this one and (b) carries an
    // `assigned_to` hint is pushed to that agent's inbox so the L3 inbox
    // step picks it up next tick, preempting the routine scan (§20.1).
    if success {
        for dep in ctx.blackboard.scan_tasks(&[]) {
            if dep.depends_on.contains(&task.id)
                && let Some(target) = dep.assigned_to
            {
                springtale_cooperation::routing::direct::assignment::assign(
                    ctx.direct_inbox,
                    target,
                    dep,
                );
            }
        }
    }

    // §13 audit-log entry — feeds next tick's
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
    ) = (success, &action)
    {
        {
            let key = format!(
                "action:{}:{}:{}:{}",
                ctx.tick.sequence, ctx.member.agent_id.0, connector, action_name
            );
            let record = serde_json::json!({
                "tick": ctx.tick.sequence,
                "agent": ctx.member.agent_id.0.to_string(),
                "duration_ms": duration_ms,
            });
            ctx.shared_env.write(&key, record, ctx.member.agent_id);

            // B4 — L0 stigmergy: deposit a `Substrate` surface tagged with
            // the connector capability so peers sense recent activity
            // (`COOPERATION.md §10`). 60s TTL — surfaces fade before stale
            // data misleads scan_and_claim.
            ctx.surfaces.deposit(
                ctx.member.agent_id,
                SurfaceType::Substrate,
                serde_json::json!({
                    "connector": connector,
                    "action": action_name,
                    "tick": ctx.tick.sequence,
                }),
                Some(std::time::Duration::from_secs(60)),
                Some(springtale_cooperation::capability::CapabilityDecl::new(
                    connector,
                )),
            );
            // Phase H5: surface deposit visible to user — drives the
            // stigmergy-trail UI marker on the colony canvas.
            springtale_cooperation::events::emit(
                ctx.cooperation_tx,
                springtale_cooperation::events::CooperationEvent::SurfaceDeposited {
                    formation_id: springtale_cooperation::types::FormationId(ctx.formation_id),
                    agent: ctx.member.agent_id,
                    surface_kind: format!("substrate:{connector}:{action_name}"),
                    ttl_ms: 60_000,
                },
            );
        }
    }

    ctx.member.active_task = None;

    ExecuteOutcome {
        action_descriptor: Some(descriptor),
        alignment: if success { 1.0 } else { 0.3 },
        consensus_task: None,
    }
}

/// If the member already has an active task carried over from a prior
/// tick, surface its descriptor on the report and stay on it. Returns
/// `None` to fall through to the new-claim path; `Some` to short-circuit
/// the executor.
///
/// Cancellation: tasks active for more than 300 ticks are auto-released
/// per Spring engine `CMobileCAI::SlowUpdate` 5-second auto-cancel.
fn continue_active_task(ctx: &ExecuteCtx<'_>) -> Option<ExecuteOutcome> {
    let active = ctx.member.active_task.as_ref()?;
    let elapsed = active.ticks_elapsed(ctx.tick.sequence);
    if active.state.is_active() && elapsed > 300 {
        ctx.blackboard.release_task(&active.task.id.to_string());
        // Continuation cancellation: clear the slot and report idle.
        // We can't mutate `member` from a `&` borrow, so signal via a
        // sentinel — the caller checks active_task on the next tick and
        // observes it gone. Caller short-circuit happens via the
        // `chosen_task.is_none() && active_task.is_some()` path in the
        // pipeline.
        return Some(ExecuteOutcome {
            action_descriptor: None,
            alignment: 0.5,
            consensus_task: None,
        });
    }
    let descriptor = crate::cooperation::task_dispatch::subtask_to_descriptor(&active.task);
    Some(ExecuteOutcome {
        action_descriptor: Some(descriptor),
        alignment: 1.0,
        consensus_task: None,
    })
}
