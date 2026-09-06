//! Rule-engine trigger dispatch — what fires when a `TriggerEvent` arrives
//! on `bot.rule_rx`. Lives in its own module so `event_loop.rs` stays
//! composition-only (the cooperation tick pipeline is in `tick_steps/`).

use crate::runtime::lifecycle::Bot;

/// Parse an approval-card callback payload: `apr:<uuid>:<y|n>`.
/// Returns `(id, approved)`; the Details button (`:d`) is handled by the
/// pending-queue lookup, not here. Shared with the typed-reply fallback
/// in `handlers.rs` (connectors without inline keyboards).
pub(crate) fn parse_approval_callback(data: &str) -> Option<(uuid::Uuid, bool)> {
    let rest = data.strip_prefix("apr:")?;
    let (id, verdict) = rest.rsplit_once(':')?;
    let id = uuid::Uuid::parse_str(id).ok()?;
    match verdict {
        "y" => Some((id, true)),
        "n" => Some((id, false)),
        _ => None,
    }
}

pub async fn handle_trigger_event(
    bot: &mut Bot,
    event: &springtale_core::rule::engine::TriggerEvent,
) -> Result<(), crate::error::BotError> {
    // W2 — approval-over-chat resume. Inline-keyboard taps arrive as
    // `callback_query_received` trigger events; resolve the pending gate
    // BEFORE rule dispatch so the paused tool loop wakes immediately.
    if event.event.as_deref() == Some("callback_query_received")
        && let Some(data) = event.payload.get("callback_data").and_then(|v| v.as_str())
        && let Some((id, approved)) = parse_approval_callback(data)
    {
        if let Some(gate) = bot.capability_bridge.approval_gate() {
            let decision = if approved {
                springtale_runtime::approval::ApprovalDecision::Approved {
                    approver: "owner (chat)".to_owned(),
                    approved_at: chrono::Utc::now(),
                }
            } else {
                springtale_runtime::approval::ApprovalDecision::Denied {
                    reason: "denied from chat".to_owned(),
                    denied_at: chrono::Utc::now(),
                }
            };
            match gate
                .resolve(
                    springtale_runtime::approval::ApprovalRequestId(id),
                    decision,
                )
                .await
            {
                Ok(()) => tracing::info!(approval = %id, approved, "chat approval resolved"),
                Err(e) => {
                    tracing::warn!(approval = %id, error = %e, "chat approval resolve failed")
                }
            }
        }
        return Ok(()); // consumed — never falls through to rule dispatch
    }
    // D1 — universal mention harvester. Runs BEFORE rule dispatch
    // so destinations get registered regardless of whether any
    // rule matches this event. The harvester is a no-op for
    // connectors without a MentionExtractor (cron / filesystem /
    // http / browser / shell).
    if let Some(connector_name) = event.connector.as_deref()
        && let Err(e) = springtale_runtime::operations::workspaces::harvest_event(
            &bot.store,
            &bot.registry,
            connector_name,
            event.event.as_deref().unwrap_or(""),
            &event.payload,
            None,
        )
        .await
    {
        tracing::warn!(
            error = %e,
            connector = %connector_name,
            "destination harvester failed (non-fatal)"
        );
    }

    let mode = match event.trigger_type.as_str() {
        "Cron" => springtale_cooperation::execution::ExecutionMode::Cron,
        "Webhook" => springtale_cooperation::execution::ExecutionMode::Webhook,
        "ConnectorEvent" => springtale_cooperation::execution::ExecutionMode::ConnectorEvent,
        "FileWatch" => springtale_cooperation::execution::ExecutionMode::FileWatch,
        _ => springtale_cooperation::execution::ExecutionMode::Manual,
    };

    // Snapshot the live formations whose members include this event's
    // connector. Their formation-scoped rules (synthesised from intent — see
    // `springtale-runtime` `operations::formation_synthesis`) only fire when we
    // dispatch with the owning formation's id, so we resolve the context here.
    let formation_ctxs: Vec<(
        springtale_cooperation::types::FormationId,
        springtale_cooperation::cadence::AgentId,
        springtale_cooperation::momentum::MomentumTier,
        springtale_cooperation::types::ApprovalPolicy,
    )> = match event.connector.as_deref() {
        Some(conn) => {
            let formations = bot.formations.read().await;
            formations
                .iter()
                .filter_map(|f| {
                    f.members
                        .iter()
                        .find(|m| m.capabilities.iter().any(|c| c.name == conn))
                        .map(|m| {
                            (
                                f.id,
                                m.agent_id,
                                f.momentum.tier,
                                f.constraints.destructive_action_policy,
                            )
                        })
                })
                .collect()
        }
        None => Vec::new(),
    };

    // Evaluate the engine once for globals, then once per relevant formation.
    let (global_matches, formation_jobs) = {
        let engine = bot.engine.read().await;
        let global_matches = springtale_core::router::dispatch::dispatch_event(&engine, event);
        let global_ids: std::collections::HashSet<_> =
            global_matches.iter().map(|m| m.rule_id).collect();

        let mut formation_jobs = Vec::new();
        for (fid, agent_id, tier, policy) in &formation_ctxs {
            for m in springtale_core::router::dispatch::dispatch_event_with_owner(
                &engine,
                event,
                None,
                Some(fid.0),
            ) {
                // `dispatch_event_with_owner` also returns Globals — skip those
                // here so they fire exactly once via the global path below.
                if !global_ids.contains(&m.rule_id) {
                    formation_jobs.push((m, *fid, *agent_id, *tier, *policy));
                }
            }
        }
        (global_matches, formation_jobs)
    };

    // Global rules — fire with a global cooperation envelope.
    for rule_match in &global_matches {
        let execution = springtale_cooperation::execution::ExecutionContext::for_global(
            rule_match.rule_id,
            mode,
        );
        dispatch_rule_match(bot, event, rule_match, execution, None).await;
    }

    // Formation-scoped rules — fire in their formation's tier context so
    // sentinel / authority gating sees the right momentum and the formation's
    // destructive-action policy. Autonomy is resolved per dispatch in
    // `dispatch_rule_match` (rule row, then the owning formation's row), which
    // overwrites the placeholder level set here.
    for (rule_match, fid, agent_id, tier, policy) in &formation_jobs {
        let execution = springtale_cooperation::execution::ExecutionContext::for_formation(
            rule_match.rule_id,
            *agent_id,
            *fid,
            *tier,
            mode,
            *policy,
            springtale_cooperation::AutonomyLevel::default(),
        );
        dispatch_rule_match(bot, event, rule_match, execution, Some(fid.0)).await;
    }
    Ok(())
}

/// Dispatch a single matched rule's actions through the connector framework
/// with the given cooperation envelope.
///
/// Every dispatch reads autonomy first (plan §6.2): the rule's own row, then
/// its owning formation's row, then `ActAutonomously`. The four levels are
/// Sheridan 1 / 4 / 5 / 7: `Observe` records what would have run and stops;
/// `Suggest` records the recommended action and stops (the human carries it
/// out via `rule run` or the RUN card); `ActWithApproval` and
/// `ActAutonomously` dispatch with the level on the envelope so the
/// approval gate reads it. Logs success / failure; never returns an error so
/// one failing rule doesn't abort the rest of the fan-out.
async fn dispatch_rule_match(
    bot: &crate::runtime::lifecycle::Bot,
    event: &springtale_core::rule::engine::TriggerEvent,
    rule_match: &springtale_core::rule::engine::RuleMatch,
    mut execution: springtale_cooperation::execution::ExecutionContext,
    formation_id: Option<uuid::Uuid>,
) {
    let formation_id = formation_id.map(|id| id.to_string());
    let autonomy = springtale_runtime::operations::agent::resolve_autonomy(
        bot.store.as_ref(),
        &rule_match.rule_id.0,
        formation_id.as_deref(),
    )
    .await;
    match autonomy {
        springtale_cooperation::AutonomyLevel::Observe => {
            tracing::info!(
                rule = %rule_match.rule_name,
                "bot: rule matched trigger — autonomy observe, holding actions"
            );
            record_autonomy_event(
                bot,
                event,
                format!(
                    "observed: rule '{}' matched, {} action(s) held",
                    rule_match.rule_name,
                    rule_match.actions.len()
                ),
            )
            .await;
            return;
        }
        springtale_cooperation::AutonomyLevel::Suggest => {
            tracing::info!(
                rule = %rule_match.rule_name,
                "bot: rule matched trigger — autonomy suggest, recording suggestion"
            );
            record_autonomy_event(
                bot,
                event,
                format!(
                    "suggested: rule '{}' — {}",
                    rule_match.rule_name,
                    summarize_actions(&rule_match.actions)
                ),
            )
            .await;
            return;
        }
        springtale_cooperation::AutonomyLevel::ActWithApproval
        | springtale_cooperation::AutonomyLevel::ActAutonomously => {}
    }
    execution.autonomy = autonomy;

    tracing::info!(
        rule = %rule_match.rule_name,
        actions = rule_match.actions.len(),
        "bot: rule matched trigger — dispatching actions"
    );
    let scope = (execution.formation_id, execution.agent_id);
    utter_rule(
        bot,
        scope,
        rule_match.rule_id,
        springtale_cooperation::utterance::UtteranceKind::Firing,
    )
    .await;
    let trigger_payload = (*rule_match.payload).clone();
    match springtale_runtime::dispatch::dispatch_actions(
        &rule_match.actions,
        &bot.capability_bridge,
        &bot.sentinel,
        execution,
        trigger_payload,
    )
    .await
    {
        Ok(chain) => tracing::info!(
            rule = %rule_match.rule_name,
            summary = %chain.brief(),
            "bot: rule actions dispatched"
        ),
        Err(e) => {
            tracing::error!(
                rule = %rule_match.rule_name,
                error = %e,
                "bot: rule actions dispatch failed"
            );
            utter_rule(
                bot,
                scope,
                rule_match.rule_id,
                springtale_cooperation::utterance::UtteranceKind::Failed,
            )
            .await;
        }
    }
}

/// Plan §1.15 E: say `kind` for a rule fire. Formation-scoped fires speak
/// as the member through the formation's `UtterCtx` (bus + observer);
/// standalone rules are observer-only, addressed by rule id.
async fn utter_rule(
    bot: &crate::runtime::lifecycle::Bot,
    scope: (
        Option<springtale_cooperation::types::FormationId>,
        Option<springtale_cooperation::cadence::AgentId>,
    ),
    rule_id: springtale_core::rule::RuleId,
    kind: springtale_cooperation::utterance::UtteranceKind,
) {
    let tick = springtale_cooperation::TickId(bot.cadence.tick_count());
    if let (Some(fid), Some(agent)) = scope {
        let mut formations = bot.formations.write().await;
        if let Some(f) = formations.iter_mut().find(|f| f.id == fid) {
            f.current_tick = tick;
            springtale_cooperation::utterance::utter(
                &mut f.utter_ctx(bot.cooperation_tx.as_ref()),
                Some(agent),
                kind,
            );
            return;
        }
    }
    springtale_cooperation::utterance::emit_solo(
        bot.cooperation_tx.as_ref(),
        &bot.utterance_defs,
        rule_id,
        tick,
        kind,
    );
}

/// Record an `observed` / `suggested` row so the canvas and event log show
/// what the rule would have done. Failure to record is logged, not fatal.
async fn record_autonomy_event(
    bot: &crate::runtime::lifecycle::Bot,
    event: &springtale_core::rule::engine::TriggerEvent,
    action_taken: String,
) {
    let entry = springtale_store::schema::events::EventEntry {
        id: uuid::Uuid::new_v4(),
        connector_name: event
            .connector
            .clone()
            .unwrap_or_else(|| "global".to_owned()),
        trigger_type: event.trigger_type.clone(),
        timestamp: chrono::Utc::now(),
        action_taken,
    };
    if let Err(e) = bot.store.log_event(&entry).await {
        tracing::warn!(error = %e, "bot: failed to record autonomy event");
    }
}

/// One-line rendering of a rule's action chain by action kind
/// (`Action` is `#[serde(tag = "type")]`, so the tag is the variant name).
fn summarize_actions(actions: &[springtale_core::rule::action::Action]) -> String {
    let kinds: Vec<String> = actions
        .iter()
        .map(|a| {
            serde_json::to_value(a)
                .ok()
                .and_then(|v| v.get("type").and_then(|t| t.as_str()).map(str::to_owned))
                .unwrap_or_else(|| "action".to_owned())
        })
        .collect();
    if kinds.is_empty() {
        "no actions".to_owned()
    } else {
        kinds.join(" -> ")
    }
}
