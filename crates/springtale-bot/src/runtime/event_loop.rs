use crate::cooperation::cadence::Tick;
use crate::runtime::lifecycle::Bot;

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

        // 1. Record tick success for momentum building
        formation.momentum.record_success();

        // 2. Persist momentum tier to config store
        let key = format!("momentum:{}", formation.id.0);
        let tier_json = serde_json::json!(formation.momentum.tier);
        if let Err(e) = bot.store.set_config(&key, &tier_json.to_string()).await {
            tracing::warn!(formation_id = %formation.id.0, error = %e, "failed to persist momentum tier");
        }

        // 3. Orchestrate — decompose intent into subtasks via AI (if available + Fever momentum)
        if formation.can_orchestrate() {
            match crate::orchestrator::orchestrate::orchestrate_formation(
                formation,
                &bot.store,
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
                    let trace_id = uuid::Uuid::new_v4();
                    for task in &subtasks {
                        if let Err(e) = formation.environment.write(
                            &task.id.to_string(),
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
