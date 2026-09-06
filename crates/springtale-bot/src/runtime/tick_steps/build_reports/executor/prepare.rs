//! Decide-phase half of the executor: everything that must run against
//! the formation before a connector is called. Claims on the blackboard
//! now (NecroDancer: "Two players cannot occupy the same tile").

use std::sync::Arc;

use tokio::sync::RwLock;

use springtale_connector::registry::store::ConnectorRegistry;
use springtale_cooperation::AutonomyLevel;
use springtale_cooperation::MomentumTier;
use springtale_cooperation::action::SubTask;
use springtale_cooperation::action_state::ActiveTask;
use springtale_cooperation::cadence::{ActionDescriptor, Tick};
use springtale_cooperation::sacrifice::SacrificeAction;
use springtale_cooperation::types::ApprovalPolicy;

use crate::cooperation::blackboard::cooperative::CooperativeBlackboard;
use crate::cooperation::blackboard::trait_::Blackboard;
use crate::cooperation::formation::FormationMember;
use crate::orchestrator::fuel::FuelBudget;

use super::ExecuteOutcome;
use super::dispatch::DispatchJob;

pub struct ExecuteCtx<'a> {
    pub formation_id: uuid::Uuid,
    pub formation_momentum: MomentumTier,
    pub destructive_policy: ApprovalPolicy,
    pub blackboard: &'a Arc<CooperativeBlackboard>,
    pub fuel: &'a FuelBudget,
    pub pacing: &'a mut springtale_cooperation::pacing::PacingManager,
    pub member: &'a mut FormationMember,
    pub tick: &'a Tick,
    pub chosen_task: Option<SubTask>,
    pub tick_action: Option<ActionDescriptor>,
    pub autonomy: AutonomyLevel,
    pub bridge: &'a springtale_runtime::CapabilityBridge,
    pub sentinel: &'a Arc<springtale_sentinel::Sentinel>,
    /// Connector registry — consulted for the manifest's advisory action
    /// hints (`read_only`) when deciding whether a task is destructive
    /// and therefore subject to the formation's destructive-action policy.
    pub registry: &'a Arc<RwLock<ConnectorRegistry>>,
    /// B9: per-agent sacrifice action returned by `agent::step::sacrifice`.
    /// When `Some`, the claim/dispatch path is skipped — for `Yield`, a
    /// yield-shaped descriptor is reported without claiming.
    pub sacrifice: Option<SacrificeAction>,
    /// B7 guard: task ids with an open consensus vote. A task in this set
    /// is skipped (no claim, no second proposal) until the vote resolves.
    pub awaiting_consensus: &'a std::collections::HashMap<uuid::Uuid, uuid::Uuid>,
    /// B7 permits: one-shot execution approvals minted by an approving
    /// vote resolution. `remove` on claim — an approval authorizes
    /// exactly one execution.
    pub consensus_approved: &'a mut std::collections::HashSet<uuid::Uuid>,
    pub cooperation_tx: Option<
        &'a tokio::sync::broadcast::Sender<springtale_cooperation::CooperationEventEnvelope>,
    >,
    /// Utterance sink for this member's claim / yield (plan §1.15).
    pub utter: springtale_cooperation::utterance::UtterCtx<'a>,
}

impl ExecuteCtx<'_> {
    /// Manifest-declared hints for the task's action, looked up the same
    /// way dispatch resolves the connector. `None` means the connector is
    /// not installed or does not declare the action — the caller treats
    /// an unknown action as destructive.
    pub async fn action_hints_for(
        &self,
        task: &SubTask,
    ) -> Option<springtale_sentinel::ActionHints> {
        let registry = self.registry.read().await;
        let entry = registry.get(&task.target_connector.name)?;
        entry
            .host
            .manifest()
            .actions
            .iter()
            .find(|decl| decl.name == task.action_name)
            .map(|decl| springtale_sentinel::ActionHints {
                read_only: decl.read_only,
                destructive: decl.destructive,
            })
    }
}

/// What `prepare` decided for one member.
pub enum Prepared {
    /// Nothing to dispatch this beat; the outcome is final.
    Settled(Box<ExecuteOutcome>),
    /// Claimed and cleared to run; `dispatch_one` takes it from here.
    Dispatch(Box<DispatchJob>),
}

/// Both variants are boxed so `Prepared` is two words wide either way.
fn settled(outcome: ExecuteOutcome) -> Prepared {
    Prepared::Settled(Box::new(outcome))
}

pub async fn prepare(mut ctx: ExecuteCtx<'_>) -> Prepared {
    // B9 short-circuit: a voluntary yield reports a yield-shaped
    // descriptor and skips claim/dispatch. `chosen_task` was already
    // cleared by the decide phase when the sacrifice fired.
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
        springtale_cooperation::utterance::utter(
            &mut ctx.utter,
            Some(sacrificer),
            springtale_cooperation::UtteranceKind::Yield { beneficiary },
        );
        return settled(ExecuteOutcome::settled(
            Some(ActionDescriptor {
                kind: "sacrifice_yield".to_owned(),
                target: Some(beneficiary.0.to_string()),
                payload_hash: 0,
            }),
            0.9,
        ));
    }

    // Continuation path: active_task carried over from a prior tick.
    if let Some(out) = continue_active_task(&ctx) {
        return settled(out);
    }

    // No new task this tick — surface the step-side descriptor only.
    let Some(task) = ctx.chosen_task.clone() else {
        return settled(ExecuteOutcome::settled(ctx.tick_action, 1.0));
    };

    // Observe: no claim, no execute. Surface the step-side action.
    if ctx.autonomy == AutonomyLevel::Observe {
        return settled(ExecuteOutcome::settled(ctx.tick_action, 1.0));
    }

    // Suggest: log + report but don't claim.
    if ctx.autonomy == AutonomyLevel::Suggest {
        tracing::debug!(
            agent = %ctx.member.agent_id.0,
            task = %task.description,
            "agent suggests task (not claiming)"
        );
        return settled(ExecuteOutcome::settled(ctx.tick_action, 1.0));
    }

    // Approve / Autonomous: pacing gate first.
    if !ctx.pacing.allow_action() {
        tracing::debug!(
            formation = %ctx.formation_id,
            agent = %ctx.member.agent_id.0,
            phase = ctx.pacing.phase_name(),
            "claim deferred — pacing rate limit hit"
        );
        return settled(ExecuteOutcome::settled(None, 0.7));
    }

    // Claim on the blackboard.
    if ctx
        .blackboard
        .claim_task(&task.id.to_string(), ctx.member.agent_id, ctx.fuel)
        .is_err()
    {
        return settled(ExecuteOutcome::settled(None, 0.5));
    }
    springtale_cooperation::utterance::utter(
        &mut ctx.utter,
        Some(ctx.member.agent_id),
        springtale_cooperation::UtteranceKind::Claimed { task: task.id },
    );

    let descriptor = crate::cooperation::task_dispatch::subtask_to_descriptor(&task);
    ctx.member.active_task = Some(ActiveTask::new(
        task.clone(),
        ctx.member.agent_id,
        ctx.tick.sequence,
    ));

    // B7 — a destructive task under `RequireConsensus` is voted on before
    // it executes, at every momentum tier. COOPERATION.pdf §3.3: the
    // destructive-action policy is "always L1" and "cooperation cannot
    // weaken" it — a momentum tier is a cooperation state, so it never
    // switches the vote off. The vote is a formation decision, so the
    // member's autonomy level does not gate it either.
    // A task whose action is unknown to the registry is destructive.
    let destructive = ctx
        .action_hints_for(&task)
        .await
        .is_none_or(|hints| !hints.read_only && hints.destructive != Some(false));
    let needs_consensus =
        destructive && matches!(ctx.destructive_policy, ApprovalPolicy::RequireConsensus);
    if needs_consensus && !ctx.consensus_approved.remove(&task.id) {
        // No one-shot approval permit for this task. Release the claim so
        // the task stays available, then either wait on the open vote or
        // open one.
        ctx.blackboard.release_task(&task.id.to_string());
        ctx.member.active_task = None;
        return settled(if ctx.awaiting_consensus.contains_key(&task.id) {
            // Vote already open — guard against re-proposing every tick.
            ExecuteOutcome::settled(None, 0.8)
        } else {
            ExecuteOutcome {
                consensus_task: Some(task),
                ..ExecuteOutcome::settled(Some(descriptor), 0.8)
            }
        });
    }

    // AlwaysRequire / ApproveOnce / AutoApprove and ActWithApproval are all
    // enforced by the sentinel gate inside dispatch_action (0.1). The
    // executor decides only consensus, which is a formation vote and
    // therefore cooperation's job. The member's action is `Requested`
    // from here until `post` records how the dispatch ended (0.3).
    if let Some(active) = ctx.member.active_task.as_mut() {
        active.request();
    }

    Prepared::Dispatch(Box::new(DispatchJob {
        agent: ctx.member.agent_id,
        formation_id: ctx.formation_id,
        formation_momentum: ctx.formation_momentum,
        destructive_policy: ctx.destructive_policy,
        autonomy: ctx.autonomy,
        task,
        descriptor,
        bridge: ctx.bridge.clone(),
        sentinel: ctx.sentinel.clone(),
        blackboard: ctx.blackboard.clone(),
    }))
}

/// If the member already has an active task carried over from a prior
/// tick, surface its descriptor on the report and stay on it. Returns
/// `None` to fall through to the new-claim path; `Some` to short-circuit.
///
/// Cancellation: tasks active for more than 300 ticks are auto-released
/// per Spring engine `CMobileCAI::SlowUpdate` 5-second auto-cancel.
fn continue_active_task(ctx: &ExecuteCtx<'_>) -> Option<ExecuteOutcome> {
    let active = ctx.member.active_task.as_ref()?;
    let elapsed = active.ticks_elapsed(ctx.tick.sequence);
    if active.state.is_active() && elapsed > 300 {
        ctx.blackboard.release_task(&active.task.id.to_string());
        return Some(ExecuteOutcome::settled(None, 0.5));
    }
    let descriptor = crate::cooperation::task_dispatch::subtask_to_descriptor(&active.task);
    Some(ExecuteOutcome::settled(Some(descriptor), 1.0))
}
