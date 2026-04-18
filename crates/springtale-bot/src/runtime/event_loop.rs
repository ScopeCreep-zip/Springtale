use std::collections::HashMap;

use crate::cooperation::blackboard::Blackboard;
use crate::cooperation::cadence::Tick;
use crate::cooperation::formation::Formation;
use crate::runtime::lifecycle::Bot;
use springtale_cooperation::awareness::gossip::{self, MemberState};
use springtale_cooperation::cadence::{AgentId, TickReport};
use springtale_cooperation::momentum::MomentumTier;
use springtale_cooperation::rally::cascade;
use springtale_cooperation::tick_processor;
use springtale_cooperation::transformation::trigger as transform_trigger;

use super::handlers::handle_incoming_message;

/// Main event loop: receives messages and trigger events, routes
/// them to handlers, and sends responses back.
///
/// Critical constraint: NEVER use `?` on individual message processing.
/// A single bad message must not crash the bot.
pub async fn run_event_loop(bot: &mut Bot) {
    loop {
        tokio::select! {
            // Source 1: Incoming chat messages from connectors
            Some(msg) = bot.connector_rx.recv() => {
                if let Err(e) = handle_incoming_message(bot, &msg).await {
                    tracing::error!(
                        user_id = %msg.user_id,
                        error = %e,
                        "message processing failed — continuing"
                    );
                }
            }

            // Source 2: Trigger events from rule engine
            Some(trigger) = bot.rule_rx.recv() => {
                if let Err(e) = handle_trigger_event(bot, &trigger).await {
                    tracing::error!(
                        error = %e,
                        "trigger processing failed — continuing"
                    );
                }
            }

            // Source 3: Cadence ticks from cooperation module (§5)
            result = bot.cadence_rx.recv() => {
                match result {
                    Ok(tick) => {
                        handle_cadence_tick(bot, &tick).await;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                        tracing::warn!(skipped = count, "cadence receiver lagged — skipping ticks");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::debug!("cadence channel closed");
                    }
                }
            }

            // Source 4: Formation commands from runtime operations
            Some(cmd) = bot.formation_cmd_rx.recv() => {
                handle_formation_command(bot, cmd).await;
            }

            // All channels closed — shutdown
            else => {
                tracing::info!("all event channels closed — bot shutting down");
                break;
            }
        }
    }
}

// Message handling (handle_incoming_message, ai_fallback, send_response)
// extracted to runtime/handlers.rs — single concern per module.

async fn handle_trigger_event(
    bot: &mut Bot,
    event: &springtale_core::rule::engine::TriggerEvent,
) -> Result<(), crate::error::BotError> {
    let engine = bot.engine.read().await;
    let matches = springtale_core::router::dispatch::dispatch_event(&engine, event);
    drop(engine);

    for rule_match in &matches {
        tracing::info!(
            rule = %rule_match.rule_name,
            actions = rule_match.actions.len(),
            "bot: rule matched trigger — dispatching actions"
        );

        for action in rule_match.actions.iter() {
            match springtale_runtime::dispatch::dispatch_action(
                action,
                &bot.registry,
                &bot.sentinel,
            )
            .await
            {
                Ok(msg) => {
                    tracing::info!(
                        rule = %rule_match.rule_name,
                        result = %msg,
                        "bot: action dispatched"
                    );
                }
                Err(e) => {
                    tracing::error!(
                        rule = %rule_match.rule_name,
                        error = %e,
                        "bot: action dispatch failed"
                    );
                }
            }
        }
    }

    Ok(())
}

// Action dispatch delegated to springtale_runtime::dispatch::dispatch_action
// — single source of truth shared between daemon and bot.

/// Handle a cadence tick — process all active formations.
///
/// Per COOPERATION.pdf §5: each tick broadcasts the current intent.
/// Formations collect tick reports from members, update momentum (§7),
/// check for interference (§13), and trigger recovery if needed (§15, §18).
async fn handle_cadence_tick(bot: &mut Bot, tick: &Tick) {
    let formations = bot.formations.read().await;
    let formation_count = formations.len();

    if formation_count == 0 {
        return; // no active formations — skip
    }

    tracing::trace!(
        tick = tick.sequence,
        formations = formation_count,
        "cadence tick processing"
    );

    drop(formations);

    let mut formations = bot.formations.write().await;
    for formation in formations.iter_mut() {
        if !formation.is_viable() {
            continue;
        }
        if formation.paused {
            continue;
        }

        // 1. Run per-agent behavioral loop (perceive → decide → act → report)
        //    Per Spring engine SlowUpdate: each agent decides what to do this tick.
        //    Per RimWorld ThinkTree: scan blackboard for work matching capabilities.
        let reports_sender = bot.cadence.reports_sender();
        let mut member_reports = run_agent_loops(formation, tick, &bot.registry, &bot.sentinel, &bot.store, &reports_sender).await;

        // 1b. Drain any async reports from the cadence reports channel (§5.4).
        //     Agents that completed work between ticks send reports via
        //     `cadence.reports_sender()`. Merge with synchronous reports so
        //     the tick processor sees a complete picture.
        while let Ok(async_report) = bot.cadence_reports_rx.try_recv() {
            member_reports.push(async_report);
        }

        // 2. Process tick (cooperation-layer: interference detection, aggregation)
        let result = tick_processor::process_tick(member_reports);

        // 3. Check momentum decay from inactivity (Microsoft AGT pattern)
        formation.momentum.check_decay();

        // 4. Update momentum from actual results (not unconditional)
        if result.all_succeeded {
            formation.momentum.record_success();
        } else if !result.interferences.is_empty() {
            for _ in &result.interferences {
                formation.momentum.record_interference();
            }
        } else {
            formation.momentum.record_failure();
        }

        // 4a. Record real activity (only when agents actually acted, not just idle ticks).
        // This keeps momentum decay working — decay tracks real work, not tick heartbeats.
        let had_real_actions = result.reports.iter().any(|r| r.action_taken.is_some());
        if had_real_actions {
            formation.momentum.record_activity();
        }

        // 4b. Track per-member consecutive failures for transformation (§14)
        for report in &result.reports {
            if let Some(member) = formation.member_mut(&report.agent_id) {
                if report.intent_alignment > 0.5 {
                    member.consecutive_failures = 0;
                } else {
                    member.consecutive_failures += 1;
                }
            }
        }

        // 4c. Update liveness per member (K8s probe pattern)
        for report in &result.reports {
            if let Some(member) = formation.member_mut(&report.agent_id) {
                member.last_report_tick = tick.sequence;
                if report.intent_alignment > 0.5 {
                    member.liveness = springtale_cooperation::supervision::Liveness::Alive;
                }
            }
        }
        // Mark members that haven't reported as suspect
        for member in &mut formation.members {
            if member.last_report_tick + 3 < tick.sequence && member.is_operational() {
                member.liveness = springtale_cooperation::supervision::Liveness::Suspect {
                    missed_ticks: (tick.sequence - member.last_report_tick) as u32,
                };
            }
        }

        // 4d. Supervisor checks — evaluate each member's health (Erlang OTP)
        let cascade_count = result.interferences.len() as u32;
        for member in &formation.members {
            if let Some(action) = formation.supervisor.check_member(
                member.agent_id,
                member.liveness,
                member.consecutive_failures,
                cascade_count,
                &formation.rally,
            ) {
                match action {
                    springtale_cooperation::supervision::SupervisionAction::TransformRole { agent } => {
                        tracing::info!(agent = %agent.0, "supervisor: transform role");
                    }
                    springtale_cooperation::supervision::SupervisionAction::RetryWithRally { agent } => {
                        tracing::info!(agent = %agent.0, "supervisor: retry with rally");
                    }
                    springtale_cooperation::supervision::SupervisionAction::TriggerReplan => {
                        tracing::warn!(formation = %formation.id.0, "supervisor: trigger replan");
                    }
                    springtale_cooperation::supervision::SupervisionAction::MarkDown { agent, since_tick } => {
                        tracing::warn!(agent = %agent.0, since_tick, "supervisor: mark down");
                    }
                    springtale_cooperation::supervision::SupervisionAction::Escalate { reason } => {
                        tracing::error!(formation = %formation.id.0, reason = %reason, "supervisor: escalation");
                    }
                }
            }
        }

        // 4e. Per-member fuel consumption
        for member in &mut formation.members {
            if member.active_task.is_some() {
                member.fuel_remaining.consume(1).ok();
            }
        }

        // 5. Persist momentum state to dedicated table
        let momentum_row = springtale_store::FormationMomentumRow {
            formation_id: formation.id.0.to_string(),
            tier: format!("{:?}", formation.momentum.tier),
            consecutive_successes: formation.momentum.consecutive_successes as i64,
            interference_count: formation.momentum.interference_count as i64,
            updated_at: chrono::Utc::now(),
        };
        if let Err(e) = bot.store.upsert_formation_momentum(&momentum_row).await {
            tracing::warn!(formation_id = %formation.id.0, error = %e, "failed to persist momentum");
        }

        // 6. Broadcast updated context to all watching members
        formation.broadcast_context();

        // 7. Update awareness gossip (Warming+ only)
        update_member_awareness(formation, &result);

        // 7b. Log interference for observability
        for event in &result.interferences {
            tracing::warn!(
                formation = %formation.id.0,
                agent_a = %event.agent_a.0,
                agent_b = %event.agent_b.0,
                severity = event.severity,
                "interference detected between agents"
            );
        }

        // 8. Evaluate pacing transitions (§22, L4D Director)
        // tick.window == tick_interval (set in CadenceBus::run). Used as the
        // per-tick elapsed duration for pacing phase transition evaluation.
        let tick_interval = tick.window;
        if let Some(transition) = formation.pacing.evaluate_transition(
            &result, &formation.momentum, tick_interval,
        ) {
            tracing::info!(
                formation = %formation.id.0,
                from = %transition.from,
                to = %transition.to,
                "pacing phase transition"
            );
        }

        // 9. Cascade detection + self-rally (§15)
        if !result.all_succeeded {
            let awareness_map: HashMap<AgentId, &springtale_cooperation::awareness::LocalAwareness> =
                formation.members.iter()
                    .filter(|m| m.is_operational())
                    .map(|m| (m.agent_id, &m.awareness))
                    .collect();

            if let Some(risk) = cascade::detect_cascade(&awareness_map, &result) {
                tracing::warn!(
                    formation = %formation.id.0,
                    risk = ?risk,
                    "cascade risk detected"
                );

                // Find a failing agent to rally around
                let failing_agent = result.reports.iter()
                    .find(|r| r.intent_alignment <= 0.5)
                    .map(|r| r.agent_id);

                if let Some(agent) = failing_agent {
                    let rally_result = cascade::attempt_self_rally(
                        &mut formation.rally,
                        &formation.attention_broker,
                        &mut formation.momentum,
                        agent,
                    );
                    match &rally_result {
                        springtale_cooperation::rally::RallyResult::StabilizedWithCost { tokens_remaining } => {
                            tracing::info!(
                                formation = %formation.id.0,
                                tokens_remaining,
                                "formation self-rallied"
                            );
                        }
                        springtale_cooperation::rally::RallyResult::EscalateToOrchestrator { reason } => {
                            tracing::error!(
                                formation = %formation.id.0,
                                reason = %reason,
                                "formation rally exhausted — escalating"
                            );
                        }
                        springtale_cooperation::rally::RallyResult::Recovered => {
                            tracing::info!(formation = %formation.id.0, "formation recovered");
                        }
                    }
                    // Persist rally state after token consumption
                    let rally_row = springtale_store::FormationRallyRow {
                        formation_id: formation.id.0.to_string(),
                        tokens_remaining: formation.rally.tokens_remaining as i64,
                        max_tokens: formation.rally.max_tokens as i64,
                    };
                    if let Err(e) = bot.store.upsert_formation_rally(&rally_row).await {
                        tracing::warn!(formation_id = %formation.id.0, error = %e, "failed to persist rally state");
                    }
                }
            }
        }

        // 9b. Recovery evaluation (§18) — when agents are distressed,
        //     nearby operational agents evaluate whether to help.
        //     Per L4D: highest priority is rescuing pinned survivors.
        {
            use springtale_cooperation::recovery::{DistressSignal, executor as recovery_exec};
            use springtale_cooperation::sacrifice::evaluator::FormationSnapshot;

            // Build distress signals from non-operational members
            let distress_signals: Vec<DistressSignal> = formation.members.iter()
                .filter_map(|m| match &m.health {
                    springtale_cooperation::types::AgentHealth::Degraded { recovery_count } => {
                        Some(DistressSignal::HealthLow {
                            agent_id: m.agent_id,
                            health_pct: 1.0 - (*recovery_count as f32 * 0.3).min(0.9),
                        })
                    }
                    springtale_cooperation::types::AgentHealth::Incapacitated => {
                        Some(DistressSignal::Incapacitated {
                            agent_id: m.agent_id,
                            bleedout_remaining: std::time::Duration::from_secs(30),
                        })
                    }
                    springtale_cooperation::types::AgentHealth::Dead { recoverable } => {
                        Some(DistressSignal::Dead {
                            agent_id: m.agent_id,
                            recoverable: *recoverable,
                        })
                    }
                    _ => None,
                })
                .collect();

            // For each distress signal, find a helper
            let snapshot = FormationSnapshot {
                member_count: formation.members.len(),
                operational_count: formation.operational_count(),
                momentum_tier: formation.momentum.tier,
                rally_tokens: formation.rally.tokens_remaining,
                capabilities: formation.members.iter()
                    .flat_map(|m| m.capabilities.iter().cloned())
                    .collect(),
                unique_capabilities: vec![],
            };

            for signal in &distress_signals {
                // Each operational member evaluates whether to help
                for member in &formation.members {
                    if !member.is_operational() { continue; }

                    let attention_snapshot = formation.attention_broker.current();
                    let eval = recovery_exec::evaluate_recovery(
                        member.agent_id,
                        &member.capabilities,
                        attention_snapshot.load(&member.agent_id),
                        signal,
                        &snapshot,
                        &member.awareness,
                        &attention_snapshot,
                    );

                    if eval.should_help {
                        if let Some(recovery_action) = &eval.action {
                            tracing::info!(
                                formation = %formation.id.0,
                                helper = %member.agent_id.0,
                                help_utility = eval.help_utility,
                                action = ?std::mem::discriminant(recovery_action),
                                "agent volunteering for recovery"
                            );
                        }
                        break; // first willing helper takes it (per MH: nearest hunter)
                    }
                }
            }
        }

        // 10. Role transformation for failing/dead members (§14)
        //    Evaluate ALL members — not just non-operational. Rule 3 (5+ failures)
        //    applies to operational agents that keep failing.
        for member in &mut formation.members {
            let caps = springtale_cooperation::capability::DynamicCapabilitySet {
                base_capabilities: member.capabilities.clone(),
                context_capabilities: vec![],
                momentum_unlocked: vec![],
                transformed_capabilities: vec![],
            };
            if let Some(transformation) = transform_trigger::evaluate_transformation(
                &member.health, &caps, member.consecutive_failures,
            ) {
                member.role = springtale_cooperation::role::apply_transformation(
                    &member.capabilities,
                    &transformation,
                );
                tracing::info!(
                    formation = %formation.id.0,
                    agent = %member.agent_id.0,
                    role = member.role.name(),
                    "agent role transformed"
                );
            }
        }

        // 11. Check consensus deadlines (§11)
        let resolved_votes = formation.consensus.check_deadlines();
        if !resolved_votes.is_empty() {
            tracing::info!(
                formation = %formation.id.0,
                count = resolved_votes.len(),
                "consensus votes resolved by deadline"
            );
        }

        // 12. Expire completed/timed-out commit barriers (§12)
        formation.active_commits.retain(|c| !c.is_expired() && !c.is_complete());

        // 13. Update mental model from this tick's observations (§21)
        springtale_cooperation::mental_model::learning::update_model(
            &mut formation.mental_model,
            &result.reports,
            &result.interferences,
            result.all_succeeded,
        );

        // 14. Orchestrate — decompose intent into subtasks via AI (if available + Fever momentum)
        if formation.can_orchestrate() {
            match crate::orchestrator::orchestrate::orchestrate_formation(
                formation,
                &bot.registry,
            )
            .await
            {
                Ok(subtasks) => {
                    tracing::info!(
                        formation = %formation.id.0,
                        subtasks = subtasks.len(),
                        "orchestrator decomposed intent into subtasks"
                    );
                    // Post subtasks to blackboard for members to pull (RimWorld pattern)
                    // Key prefix "task:" enables scan_tasks() to find them
                    let trace_id = uuid::Uuid::new_v4();
                    for task in &subtasks {
                        let task_key = format!("task:{}", task.id);
                        if let Err(e) = formation.environment.write(
                            &task_key,
                            serde_json::to_value(task).unwrap_or_default(),
                            trace_id,
                            &formation.fuel,
                        ) {
                            tracing::warn!(task = %task.id, error = %e, "failed to post subtask to blackboard");
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(formation = %formation.id.0, error = %e, "orchestration failed");
                    formation.momentum.record_failure();
                }
            }
        }
    }

    // Reclaim slots from permanently dead members (L4D pattern:
    // recoverable-dead stay for peer revive, permanently dead get removed)
    for formation in formations.iter_mut() {
        let removed = formation.remove_dead_members();
        if removed > 0 {
            tracing::info!(
                formation = %formation.id.0,
                removed,
                "reclaimed slots from dead members"
            );
        }
    }

    // Remove non-viable formations (no operational members left)
    formations.retain(|f| f.is_viable());
}

/// Run the per-agent behavioral loop for each formation member.
///
/// Per Spring engine CMobileCAI::SlowUpdate():
/// - If no active task → scan blackboard for work
/// - If has task → continue executing
///
/// Per RimWorld ThinkTree:
/// - Agent scans world for valid work matching capabilities
/// - Claims task to prevent redundancy
/// - Executes via connector dispatch
///
/// Returns TickReports with REAL action data (not fake None reports).
async fn run_agent_loops(
    formation: &mut Formation,
    tick: &Tick,
    registry: &std::sync::Arc<tokio::sync::RwLock<springtale_connector::registry::store::ConnectorRegistry>>,
    sentinel: &std::sync::Arc<springtale_sentinel::Sentinel>,
    store: &std::sync::Arc<dyn springtale_store::StorageBackend>,
    reports_sender: &tokio::sync::mpsc::Sender<TickReport>,
) -> Vec<TickReport> {
    use springtale_cooperation::action_state::ActiveTask;
    use springtale_cooperation::agent_loop::{self, AgentDecision};

    let mut reports = Vec::new();

    // Scan blackboard for available tasks (shared across all members)
    let available_tasks = formation.environment.scan_tasks(&[]);

    for member in &mut formation.members {
        if !member.is_operational() {
            continue;
        }

        // Get agent's autonomy level from store (per 0 A.D. stance system)
        let agent_name = member.agent_id.0.to_string();
        let autonomy_str = springtale_runtime::operations::agent::get_autonomy(
            store.as_ref(), &agent_name,
        ).await.unwrap_or_else(|_| "suggest".to_owned());
        let autonomy = springtale_cooperation::AutonomyLevel::parse(&autonomy_str);

        // DECIDE: what should this agent do?
        let member_attention = formation.attention_broker.current().load(&member.agent_id);
        let decision = agent_loop::decide_agent_tick(
            member.agent_id,
            &member.capabilities,
            &member.active_task,
            &available_tasks,
            member_attention,
            autonomy,
            tick.sequence,
        );

        // ACT: execute the decision
        let (action_descriptor, alignment) = match decision {
            AgentDecision::Idle => (None, 1.0),

            AgentDecision::ContinueExecuting => {
                // Task still running — report what it's doing
                let desc = member.active_task.as_ref()
                    .map(|t| crate::cooperation::task_dispatch::subtask_to_descriptor(&t.task));
                (desc, 1.0)
            }

            AgentDecision::AdvanceToRequested => {
                if let Some(ref mut task) = member.active_task {
                    task.request();
                }
                (None, 1.0)
            }

            AgentDecision::PromoteToExecuting => {
                if let Some(ref mut task) = member.active_task {
                    task.begin_execution();
                }
                let desc = member.active_task.as_ref()
                    .map(|t| crate::cooperation::task_dispatch::subtask_to_descriptor(&t.task));
                (desc, 1.0)
            }

            AgentDecision::WaitForApproval => (None, 0.8),

            AgentDecision::HandleCancellation => {
                // Clean up: release task back to blackboard, clear active
                if let Some(ref task) = member.active_task {
                    formation.environment.release_task(&task.task.id.to_string());
                }
                member.active_task = None;
                (None, 0.5) // partial alignment — was interrupted
            }

            AgentDecision::Suggest(task) => {
                tracing::debug!(
                    agent = %member.agent_id.0,
                    task = %task.description,
                    "agent suggests task (not claiming)"
                );
                (None, 1.0)
            }

            AgentDecision::ClaimTask { task, auto_execute } => {
                // Claim the task on the blackboard
                let claim_result = formation.environment.claim_task(
                    &task.id.to_string(),
                    member.agent_id,
                    &formation.fuel,
                );

                if claim_result.is_ok() {
                    let descriptor = crate::cooperation::task_dispatch::subtask_to_descriptor(&task);
                    let active = ActiveTask::new(task.clone(), member.agent_id, tick.sequence);
                    member.active_task = Some(active);

                    if auto_execute {
                        // Autonomous: immediately start executing
                        if let Some(ref mut active) = member.active_task {
                            active.request();
                            active.begin_execution();
                        }

                        // Execute the task via connector dispatch
                        let action = crate::cooperation::task_dispatch::subtask_to_action(&task);
                        let exec_start = std::time::Instant::now();
                        let exec_result = springtale_runtime::dispatch::dispatch_action(
                            &action, registry, sentinel,
                        ).await;

                        let duration_ms = exec_start.elapsed().as_millis() as u64;

                        // Extract result data before consuming
                        let (success, output) = match &exec_result {
                            Ok(msg) => (true, serde_json::json!({"result": msg})),
                            Err(err) => (false, serde_json::json!({"error": err})),
                        };
                        let error_msg = exec_result.err();

                        // Update active task state
                        if let Some(ref mut active) = member.active_task {
                            if success {
                                active.succeed();
                            } else {
                                active.fail(error_msg.unwrap_or_default());
                            }
                        }

                        // Post result to blackboard
                        let sub_result = springtale_cooperation::SubTaskResult {
                            task_id: task.id,
                            agent_id: member.agent_id,
                            success,
                            output,
                            duration_ms,
                        };
                        let _ = formation.environment.post_result(&sub_result, &formation.fuel);

                        // Clear completed task
                        member.active_task = None;

                        (Some(descriptor), if success { 1.0 } else { 0.3 })
                    } else {
                        // Approval mode: claim but don't execute
                        if let Some(ref mut active) = member.active_task {
                            active.request();
                        }
                        (None, 0.8)
                    }
                } else {
                    // Claim failed (fuel exhausted or race condition)
                    (None, 0.5)
                }
            }
        };

        let report = TickReport {
            agent_id: member.agent_id,
            tick_sequence: tick.sequence,
            action_taken: action_descriptor,
            latency: std::time::Duration::from_millis(0),
            intent_alignment: alignment,
            interference_with: vec![],
        };

        // Send to the async reports channel so external consumers (tests,
        // observers, cross-formation coordinators) can also read reports.
        let _ = reports_sender.try_send(report.clone());
        reports.push(report);
    }

    reports
}

/// Update each member's awareness with neighbor snapshots.
///
/// Bot-layer responsibility: only this code has access to FormationMember
/// fields. Delegates to cooperation crate's gossip module for the actual
/// snapshot distribution logic.
fn update_member_awareness(
    formation: &mut Formation,
    result: &tick_processor::FormationTickResult,
) {
    if formation.momentum.tier == MomentumTier::Cold {
        return;
    }

    // Build per-agent success lookup from tick reports
    let success_map: HashMap<AgentId, bool> = result
        .reports
        .iter()
        .map(|r| (r.agent_id, r.intent_alignment > 0.5))
        .collect();

    // Build MemberState views for the gossip module
    let mut member_states: Vec<MemberState<'_>> = formation
        .members
        .iter_mut()
        .filter(|m| m.is_operational())
        .map(|m| {
            let last_success = success_map.get(&m.agent_id).copied().unwrap_or(true);
            MemberState {
                agent_id: m.agent_id,
                awareness: &mut m.awareness,
                health: &m.health,
                role_name: m.role.name().to_owned(),
                fuel_pct: if formation.fuel.initial() > 0 {
                    formation.fuel.remaining() as f32 / formation.fuel.initial() as f32
                } else {
                    1.0
                },
                attention_load: formation.attention_broker.current().load(&m.agent_id),
                last_success,
            }
        })
        .collect();

    gossip::update_awareness(&mut member_states, &result.reports, formation.momentum.tier);
}

/// Handle a formation command from runtime operations.
///
/// This is the ONLY code path that materializes live Formation structs
/// from database rows, or removes them from the active set.
async fn handle_formation_command(
    bot: &mut Bot,
    cmd: springtale_cooperation::command::FormationCommand,
) {
    use springtale_cooperation::command::FormationCommand;

    match cmd {
        FormationCommand::Deploy { formation_id } => {
            match crate::cooperation::lifecycle::spawn_formation(
                &formation_id.to_string(),
                &bot.store,
                &bot.registry,
            )
            .await
            {
                Ok(formation) => {
                    bot.formations.write().await.push(formation);
                    tracing::info!(id = %formation_id, "formation materialized from DB");
                }
                Err(e) => {
                    tracing::error!(id = %formation_id, error = %e, "formation spawn failed");
                }
            }
        }
        FormationCommand::Dissolve {
            formation_id,
            reason,
        } => {
            let mut formations = bot.formations.write().await;
            let before = formations.len();
            formations.retain(|f| f.id != formation_id);
            let removed = before - formations.len();
            if removed > 0 {
                tracing::info!(id = %formation_id, %reason, "formation dissolved");
            } else {
                tracing::warn!(id = %formation_id, "formation not found for dissolve");
            }
        }
        FormationCommand::Pause { formation_id } => {
            let mut formations = bot.formations.write().await;
            if let Some(formation) = formations.iter_mut().find(|f| f.id == formation_id) {
                formation.paused = true;
                formation.broadcast_context();
                tracing::info!(id = %formation_id, "formation paused");
            } else {
                tracing::warn!(id = %formation_id, "formation not found for pause");
            }
        }
        FormationCommand::Resume { formation_id } => {
            let mut formations = bot.formations.write().await;
            if let Some(formation) = formations.iter_mut().find(|f| f.id == formation_id) {
                formation.paused = false;
                formation.broadcast_context();
                tracing::info!(id = %formation_id, "formation resumed");
            } else {
                tracing::warn!(id = %formation_id, "formation not found for resume");
            }
        }
        FormationCommand::ChangeIntent {
            formation_id,
            intent,
        } => {
            let mut formations = bot.formations.write().await;
            if let Some(formation) = formations
                .iter_mut()
                .find(|f| f.id == formation_id)
            {
                formation.intent = intent;
                tracing::info!(id = %formation_id, "formation intent updated");
            } else {
                tracing::warn!(id = %formation_id, "formation not found for intent change");
            }
        }
        FormationCommand::AddMember {
            formation_id,
            connector_name,
        } => {
            let mut formations = bot.formations.write().await;
            if let Some(formation) = formations.iter_mut().find(|f| f.id == formation_id) {
                let agent_id = springtale_cooperation::cadence::AgentId::new();
                let member = crate::cooperation::formation::FormationMember::from_strings(
                    agent_id,
                    vec![connector_name.clone()],
                );
                formation.join(member);
                tracing::info!(id = %formation_id, connector = %connector_name, "member added to formation");
            } else {
                tracing::warn!(id = %formation_id, "formation not found for AddMember");
            }
        }
        FormationCommand::RemoveMember {
            formation_id,
            connector_name,
        } => {
            let mut formations = bot.formations.write().await;
            if let Some(formation) = formations.iter_mut().find(|f| f.id == formation_id) {
                // Find the member whose capabilities include this connector
                if let Some(agent_id) = formation
                    .members
                    .iter()
                    .find(|m| m.capabilities.iter().any(|c| c == &connector_name))
                    .map(|m| m.agent_id)
                {
                    formation.leave(agent_id);
                    tracing::info!(
                        id = %formation_id,
                        connector = %connector_name,
                        "member removed from formation"
                    );
                } else {
                    tracing::warn!(
                        id = %formation_id,
                        connector = %connector_name,
                        "member not found for RemoveMember"
                    );
                }
            } else {
                tracing::warn!(id = %formation_id, "formation not found for RemoveMember");
            }
        }
        FormationCommand::Rally { formation_id } => {
            let mut formations = bot.formations.write().await;
            if let Some(formation) = formations
                .iter_mut()
                .find(|f| f.id == formation_id)
            {
                // Manual rally — find the lowest-performing agent and rally around them
                let attn = formation.attention_broker.current();
                let weakest = formation
                    .members
                    .iter()
                    .filter(|m| m.is_operational())
                    .min_by(|a, b| {
                        let la = attn.load(&a.agent_id);
                        let lb = attn.load(&b.agent_id);
                        la.partial_cmp(&lb).unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|m| m.agent_id);

                if let Some(agent) = weakest {
                    let rally_result = cascade::attempt_self_rally(
                        &mut formation.rally,
                        &formation.attention_broker,
                        &mut formation.momentum,
                        agent,
                    );
                    match &rally_result {
                        springtale_cooperation::rally::RallyResult::StabilizedWithCost {
                            tokens_remaining,
                        } => {
                            tracing::info!(
                                id = %formation_id,
                                tokens_remaining,
                                "manual rally: formation stabilized"
                            );
                            // Persist rally state
                            let rally_row = springtale_store::FormationRallyRow {
                                formation_id: formation.id.0.to_string(),
                                tokens_remaining: formation.rally.tokens_remaining as i64,
                                max_tokens: formation.rally.max_tokens as i64,
                            };
                            if let Err(e) =
                                bot.store.upsert_formation_rally(&rally_row).await
                            {
                                tracing::warn!(error = %e, "failed to persist rally state");
                            }
                        }
                        springtale_cooperation::rally::RallyResult::EscalateToOrchestrator {
                            reason,
                        } => {
                            tracing::error!(
                                id = %formation_id,
                                reason = %reason,
                                "manual rally: exhausted"
                            );
                        }
                        springtale_cooperation::rally::RallyResult::Recovered => {
                            tracing::info!(id = %formation_id, "manual rally: recovered");
                        }
                    }
                } else {
                    tracing::warn!(id = %formation_id, "manual rally: no operational members");
                }
            } else {
                tracing::warn!(id = %formation_id, "formation not found for rally");
            }
        }
    }
}
