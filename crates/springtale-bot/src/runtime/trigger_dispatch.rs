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
    )> = match event.connector.as_deref() {
        Some(conn) => {
            let formations = bot.formations.read().await;
            formations
                .iter()
                .filter_map(|f| {
                    f.members
                        .iter()
                        .find(|m| m.capabilities.iter().any(|c| c.name == conn))
                        .map(|m| (f.id, m.agent_id, f.momentum.tier))
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
        for (fid, agent_id, tier) in &formation_ctxs {
            for m in springtale_core::router::dispatch::dispatch_event_with_owner(
                &engine,
                event,
                None,
                Some(fid.0),
            ) {
                // `dispatch_event_with_owner` also returns Globals — skip those
                // here so they fire exactly once via the global path below.
                if !global_ids.contains(&m.rule_id) {
                    formation_jobs.push((m, *fid, *agent_id, *tier));
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
        dispatch_rule_match(bot, rule_match, execution).await;
    }

    // Formation-scoped rules — fire in their formation's tier context so
    // sentinel / authority gating sees the right momentum.
    for (rule_match, fid, agent_id, tier) in &formation_jobs {
        let execution = springtale_cooperation::execution::ExecutionContext::for_formation(
            rule_match.rule_id,
            *agent_id,
            *fid,
            *tier,
            mode,
        );
        dispatch_rule_match(bot, rule_match, execution).await;
    }
    Ok(())
}

/// Dispatch a single matched rule's actions through the connector framework
/// with the given cooperation envelope. Logs success / failure; never returns
/// an error so one failing rule doesn't abort the rest of the fan-out.
async fn dispatch_rule_match(
    bot: &crate::runtime::lifecycle::Bot,
    rule_match: &springtale_core::rule::engine::RuleMatch,
    execution: springtale_cooperation::execution::ExecutionContext,
) {
    tracing::info!(
        rule = %rule_match.rule_name,
        actions = rule_match.actions.len(),
        "bot: rule matched trigger — dispatching actions"
    );
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
        Err(e) => tracing::error!(
            rule = %rule_match.rule_name,
            error = %e,
            "bot: rule actions dispatch failed"
        ),
    }
}
