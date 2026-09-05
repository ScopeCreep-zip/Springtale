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

use tokio::sync::RwLock;

use springtale_connector::registry::store::ConnectorRegistry;
use springtale_cooperation::AutonomyLevel;
use springtale_cooperation::MomentumTier;
use springtale_cooperation::action::SubTask;
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
    /// Connector registry — consulted for the manifest's advisory action
    /// hints (`read_only`) when deciding whether a task is destructive
    /// and therefore subject to the formation's destructive-action policy.
    pub registry: &'a Arc<RwLock<ConnectorRegistry>>,
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

impl ExecuteCtx<'_> {
    /// Manifest-declared hints for the task's action, looked up the same
    /// way dispatch resolves the connector: registry entry by connector
    /// name, then the `ActionDecl` by action name. `None` means the
    /// connector is not installed or does not declare the action — the
    /// caller must treat an unknown action as destructive.
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
        return if ctx.awaiting_consensus.contains_key(&task.id) {
            // Vote already open — guard against re-proposing every tick.
            ExecuteOutcome {
                action_descriptor: None,
                alignment: 0.8,
                consensus_task: None,
            }
        } else {
            ExecuteOutcome {
                action_descriptor: Some(descriptor),
                alignment: 0.8,
                consensus_task: Some(task),
            }
        };
    }

    // AlwaysRequire / ApproveOnce / AutoApprove and ActWithApproval are all
    // enforced by the sentinel gate inside dispatch_action (0.1). The
    // executor no longer decides approval; it only decides consensus,
    // which is a formation vote and therefore cooperation's job.

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
        ctx.destructive_policy,
        ctx.autonomy,
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    //! End-to-end §11 consensus round-trip through the REAL executor +
    //! resolution path (plan verification: "confirm a RequireConsensus
    //! action round-trips propose → approve → execute"):
    //!
    //! 1. Formation earns Fever momentum and Peak pacing (capabilities
    //!    are earned, not granted — §7/§22).
    //! 2. Executor call #1 on a destructive task → opens a consensus
    //!    proposal instead of executing (B7 gate).
    //! 3. Executor call #2 → pending-vote guard: no duplicate proposal.
    //! 4. All members approve → `resolve_consensus` mints the one-shot
    //!    permit.
    //! 5. Executor call #3 → permit consumed, the connector actually
    //!    runs EXACTLY once, result posted to the blackboard.

    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    use async_trait::async_trait;
    use serde_json::json;
    use tokio::sync::RwLock;

    use springtale_connector::ConnectorError;
    use springtale_connector::capability::grant::CapabilityPolicy;
    use springtale_connector::connector::subscription::{Subscription, SubscriptionId};
    use springtale_connector::connector::trait_::{ActionResult, Connector, EventHandler};
    use springtale_connector::manifest::SignatureAlgorithm;
    use springtale_connector::manifest::types::{ActionDecl, ConnectorManifest, TriggerDecl};
    use springtale_connector::registry::store::ConnectorRegistry;
    use springtale_cooperation::TickId;
    use springtale_cooperation::action::SubTask;
    use springtale_cooperation::cadence::{AgentId, IntentPattern, Tick, TickReport};
    use springtale_cooperation::consensus::{DecisionDescriptor, DecisionSubject, VoteChoice};
    use springtale_cooperation::momentum::MomentumTier;
    use springtale_cooperation::pacing::{PacingManager, PacingPhase};
    use springtale_cooperation::tick_processor::FormationTickResult;
    use springtale_cooperation::types::{ApprovalPolicy, FormationConstraints};
    use springtale_runtime::CapabilityBridge;
    use springtale_sentinel::{Sentinel, SentinelConfig};
    use springtale_store::backend::InMemoryBackend;

    use crate::cooperation::blackboard::trait_::Blackboard;
    use crate::cooperation::formation::{Formation, FormationMember};
    use crate::runtime::tick_steps::resolve_consensus;

    use super::{ExecuteCtx, execute};

    /// Mock destructive-capable connector that counts real executions.
    struct CountingConnector {
        manifest: ConnectorManifest,
        executions: Arc<AtomicUsize>,
    }

    impl CountingConnector {
        fn new(executions: Arc<AtomicUsize>) -> Self {
            Self::with_read_only(executions, false)
        }

        fn with_read_only(executions: Arc<AtomicUsize>, read_only: bool) -> Self {
            Self {
                manifest: ConnectorManifest {
                    name: "consensus-target".into(),
                    version: "0.1.0".into(),
                    author: "test".into(),
                    description: "counts executions for the consensus round-trip".into(),
                    capabilities: vec![],
                    triggers: vec![TriggerDecl {
                        name: "ping".into(),
                        description: "ping".into(),
                        schema: None,
                    }],
                    actions: vec![ActionDecl {
                        name: "wipe".into(),
                        description: "destructive action gated by consensus".into(),
                        input_schema: None,
                        output_schema: None,
                        read_only,
                        destructive: None,
                    }],
                    data_disclosure: vec![],
                    roles: vec![],
                    wasm_hash: None,
                    signature_alg: SignatureAlgorithm::default(),
                    signature: None,
                },
                executions,
            }
        }
    }

    #[async_trait]
    impl Connector for CountingConnector {
        fn triggers(&self) -> &[TriggerDecl] {
            &self.manifest.triggers
        }
        fn actions(&self) -> &[ActionDecl] {
            &self.manifest.actions
        }
        async fn execute(
            &self,
            action: &str,
            _input: serde_json::Value,
        ) -> Result<ActionResult, ConnectorError> {
            self.executions.fetch_add(1, Ordering::SeqCst);
            Ok(ActionResult {
                success: true,
                output: json!({ "executed": action }),
                message: "consensus-approved execution".into(),
            })
        }
        async fn on_event(
            &self,
            trigger: &str,
            _handler: EventHandler,
        ) -> Result<Subscription, ConnectorError> {
            Ok(Subscription {
                id: SubscriptionId(0),
                trigger: trigger.to_owned(),
            })
        }
        async fn remove_event(&self, _sub: &Subscription) -> Result<(), ConnectorError> {
            Ok(())
        }
        fn manifest(&self) -> &ConnectorManifest {
            &self.manifest
        }
    }

    fn successful_tick_result(agent: AgentId) -> FormationTickResult {
        FormationTickResult {
            reports: vec![TickReport {
                agent_id: agent,
                tick_sequence: TickId(1),
                action_taken: Some(springtale_cooperation::cadence::ActionDescriptor {
                    kind: "work".into(),
                    target: None,
                    payload_hash: 0,
                }),
                latency: Duration::from_millis(1),
                intent_alignment: 1.0,
                interference_with: vec![],
            }],
            interferences: vec![],
            all_succeeded: true,
        }
    }

    fn make_tick() -> Tick {
        Tick {
            sequence: TickId(1),
            timestamp: Instant::now(),
            window: Duration::from_millis(33),
        }
    }

    fn destructive_task() -> SubTask {
        SubTask {
            id: uuid::Uuid::new_v4(),
            target_connector: "consensus-target".into(),
            action_name: "wipe".into(),
            params: json!({}),
            priority: 1,
            assigned_to: None,
            description: "destructive action".into(),
            depends_on: vec![],
        }
    }

    #[tokio::test]
    async fn consensus_round_trip_propose_guard_approve_execute_once() {
        // ── Arrange: real bridge + sentinel + formation ──
        let executions = Arc::new(AtomicUsize::new(0));
        let mut registry = ConnectorRegistry::new(CapabilityPolicy::AllowAll);
        registry
            .install_native(Box::new(CountingConnector::new(executions.clone())))
            .unwrap();
        let registry = Arc::new(RwLock::new(registry));
        let bridge = CapabilityBridge::new(registry.clone());
        let store: Arc<dyn springtale_store::StorageBackend> = Arc::new(InMemoryBackend::new());
        let sentinel = Arc::new(Sentinel::new(SentinelConfig::default(), store));

        let members: Vec<FormationMember> = (0..3)
            .map(|_| FormationMember::from_strings(AgentId::new(), vec!["consensus-target".into()]))
            .collect();
        let voters: Vec<AgentId> = members.iter().map(|m| m.agent_id).collect();
        let constraints = FormationConstraints {
            destructive_action_policy: ApprovalPolicy::RequireConsensus,
            ..FormationConstraints::default()
        };
        let mut formation = Formation::new_disconnected(
            members,
            IntentPattern::Execute { plan_id: None },
            constraints,
        );

        // ── Earn the capabilities (§7 Fever + §22 Peak — not granted) ──
        for _ in 0..15 {
            formation.momentum.record_success();
        }
        assert_eq!(formation.momentum.tier, MomentumTier::Fever);

        let mut pacing = PacingManager::default();
        let driver = voters[0];
        for _ in 0..16 {
            pacing.evaluate_transition(
                &successful_tick_result(driver),
                &formation.momentum,
                Duration::from_millis(33),
            );
        }
        assert!(
            matches!(pacing.current_phase, PacingPhase::Peak { .. }),
            "formation earned Peak pacing (full tick rate + 30 actions/min)"
        );

        let task = destructive_task();
        let tick = make_tick();
        let blackboard = formation.blackboard.clone();
        let shared_env = formation.shared_env.clone();
        let surfaces = formation.surfaces.clone();
        let fuel = formation.fuel.clone();
        let direct_inbox = formation.direct_inbox.clone();
        let mut member = formation.members[0].clone();

        // ── Call #1: destructive task opens a proposal, doesn't execute ──
        let outcome = execute(ExecuteCtx {
            formation_id: formation.id.0,
            formation_momentum: MomentumTier::Fever,
            destructive_policy: ApprovalPolicy::RequireConsensus,
            blackboard: blackboard.as_ref(),
            shared_env: shared_env.as_ref(),
            surfaces: surfaces.as_ref(),
            fuel: fuel.as_ref(),
            pacing: &mut pacing,
            member: &mut member,
            tick: &tick,
            chosen_task: Some(task.clone()),
            tick_action: None,
            autonomy: springtale_cooperation::AutonomyLevel::ActAutonomously,
            bridge: &bridge,
            sentinel: &sentinel,
            registry: &registry,
            direct_inbox: direct_inbox.as_ref(),
            sacrifice: None,
            awaiting_consensus: &formation.awaiting_consensus,
            consensus_approved: &mut formation.consensus_approved,
            cooperation_tx: None,
        })
        .await;
        let proposal = outcome.consensus_task.expect("call #1 opens a proposal");
        assert_eq!(executions.load(Ordering::SeqCst), 0, "nothing executed yet");

        // agent_pipeline's post-loop block: propose + register the guard.
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
        let outcome = execute(ExecuteCtx {
            formation_id: formation.id.0,
            formation_momentum: MomentumTier::Fever,
            destructive_policy: ApprovalPolicy::RequireConsensus,
            blackboard: blackboard.as_ref(),
            shared_env: shared_env.as_ref(),
            surfaces: surfaces.as_ref(),
            fuel: fuel.as_ref(),
            pacing: &mut pacing,
            member: &mut member,
            tick: &tick,
            chosen_task: Some(task.clone()),
            tick_action: None,
            autonomy: springtale_cooperation::AutonomyLevel::ActAutonomously,
            bridge: &bridge,
            sentinel: &sentinel,
            registry: &registry,
            direct_inbox: direct_inbox.as_ref(),
            sacrifice: None,
            awaiting_consensus: &formation.awaiting_consensus,
            consensus_approved: &mut formation.consensus_approved,
            cooperation_tx: None,
        })
        .await;
        assert!(
            outcome.consensus_task.is_none(),
            "guard prevents a second proposal for the same task"
        );
        assert_eq!(executions.load(Ordering::SeqCst), 0);

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
        let outcome = execute(ExecuteCtx {
            formation_id: formation.id.0,
            formation_momentum: MomentumTier::Fever,
            destructive_policy: ApprovalPolicy::RequireConsensus,
            blackboard: blackboard.as_ref(),
            shared_env: shared_env.as_ref(),
            surfaces: surfaces.as_ref(),
            fuel: fuel.as_ref(),
            pacing: &mut pacing,
            member: &mut member,
            tick: &tick,
            chosen_task: Some(task.clone()),
            tick_action: None,
            autonomy: springtale_cooperation::AutonomyLevel::ActAutonomously,
            bridge: &bridge,
            sentinel: &sentinel,
            registry: &registry,
            direct_inbox: direct_inbox.as_ref(),
            sacrifice: None,
            awaiting_consensus: &formation.awaiting_consensus,
            consensus_approved: &mut formation.consensus_approved,
            cooperation_tx: None,
        })
        .await;
        assert!(
            outcome.consensus_task.is_none(),
            "no re-vote after approval"
        );
        assert_eq!(
            executions.load(Ordering::SeqCst),
            1,
            "the destructive action executed EXACTLY once"
        );
        assert!(
            formation.consensus_approved.is_empty(),
            "the permit was one-shot — consumed on execution"
        );
        assert!(
            blackboard.read_result(task.id).is_some(),
            "result posted to the blackboard"
        );
    }

    /// Cold-tier `RequireConsensus` formation over one connector whose
    /// single action `wipe` carries the given `read_only` manifest hint.
    fn cold_consensus_fixture(
        read_only: bool,
    ) -> (
        Arc<AtomicUsize>,
        Arc<RwLock<ConnectorRegistry>>,
        CapabilityBridge,
        Arc<Sentinel>,
        Formation,
    ) {
        let executions = Arc::new(AtomicUsize::new(0));
        let mut registry = ConnectorRegistry::new(CapabilityPolicy::AllowAll);
        registry
            .install_native(Box::new(CountingConnector::with_read_only(
                executions.clone(),
                read_only,
            )))
            .unwrap();
        let registry = Arc::new(RwLock::new(registry));
        let bridge = CapabilityBridge::new(registry.clone());
        let store: Arc<dyn springtale_store::StorageBackend> = Arc::new(InMemoryBackend::new());
        let sentinel = Arc::new(Sentinel::new(SentinelConfig::default(), store));
        let members: Vec<FormationMember> = (0..3)
            .map(|_| FormationMember::from_strings(AgentId::new(), vec!["consensus-target".into()]))
            .collect();
        let formation = Formation::new_disconnected(
            members,
            IntentPattern::Execute { plan_id: None },
            FormationConstraints {
                destructive_action_policy: ApprovalPolicy::RequireConsensus,
                ..FormationConstraints::default()
            },
        );
        assert_eq!(formation.momentum.tier, MomentumTier::Cold);
        (executions, registry, bridge, sentinel, formation)
    }

    /// One executor call for an autonomous member at Cold tier.
    async fn execute_at_cold(
        formation: &mut Formation,
        registry: &Arc<RwLock<ConnectorRegistry>>,
        bridge: &CapabilityBridge,
        sentinel: &Arc<Sentinel>,
        member: &mut FormationMember,
        task: &SubTask,
    ) -> super::ExecuteOutcome {
        let tick = make_tick();
        let mut pacing = PacingManager::default();
        let blackboard = formation.blackboard.clone();
        let shared_env = formation.shared_env.clone();
        let surfaces = formation.surfaces.clone();
        let fuel = formation.fuel.clone();
        let direct_inbox = formation.direct_inbox.clone();
        execute(ExecuteCtx {
            formation_id: formation.id.0,
            formation_momentum: MomentumTier::Cold,
            destructive_policy: ApprovalPolicy::RequireConsensus,
            blackboard: blackboard.as_ref(),
            shared_env: shared_env.as_ref(),
            surfaces: surfaces.as_ref(),
            fuel: fuel.as_ref(),
            pacing: &mut pacing,
            member,
            tick: &tick,
            chosen_task: Some(task.clone()),
            tick_action: None,
            autonomy: springtale_cooperation::AutonomyLevel::ActAutonomously,
            bridge,
            sentinel,
            registry,
            direct_inbox: direct_inbox.as_ref(),
            sacrifice: None,
            awaiting_consensus: &formation.awaiting_consensus,
            consensus_approved: &mut formation.consensus_approved,
            cooperation_tx: None,
        })
        .await
    }

    #[tokio::test]
    async fn test_execute_cold_destructive_require_consensus_proposes_without_dispatch() {
        let (executions, registry, bridge, sentinel, mut formation) = cold_consensus_fixture(false);
        let task = destructive_task();
        let mut member = formation.members[0].clone();
        let other = formation.members[1].agent_id;

        let outcome = execute_at_cold(
            &mut formation,
            &registry,
            &bridge,
            &sentinel,
            &mut member,
            &task,
        )
        .await;

        let proposal = outcome
            .consensus_task
            .expect("Cold tier opens a vote — momentum never skips consensus");
        assert_eq!(proposal.id, task.id);
        assert_eq!(
            executions.load(Ordering::SeqCst),
            0,
            "no dispatch without a vote"
        );
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
        let (executions, registry, bridge, sentinel, mut formation) = cold_consensus_fixture(true);
        let task = destructive_task();
        let mut member = formation.members[0].clone();

        let outcome = execute_at_cold(
            &mut formation,
            &registry,
            &bridge,
            &sentinel,
            &mut member,
            &task,
        )
        .await;

        assert!(
            outcome.consensus_task.is_none(),
            "read-only manifest hint: nothing to vote on"
        );
        assert_eq!(
            executions.load(Ordering::SeqCst),
            1,
            "dispatched immediately"
        );
    }
}
