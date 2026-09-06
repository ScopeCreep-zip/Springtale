//! Bot event loop — composition only.
//!
//! Per `pure-noodling-biscuit.md` lines 1843–1849 (the cooperation refactor
//! design contract): this file is a `tokio::select!` over the four event
//! sources plus a thin per-formation `tick_steps::run_tick` composition.
//! Every per-tick concern lives in its own `runtime/tick_steps/<step>.rs`
//! so each stage can be unit-tested in isolation against mocked deps.
//!
//! Critical constraint: NEVER use `?` on individual message processing.
//! A single bad message must not crash the bot; log the error, continue.

use crate::cooperation::cadence::Tick;
use crate::runtime::lifecycle::Bot;
use crate::runtime::tick_steps;
use crate::runtime::trigger_dispatch::handle_trigger_event;

use super::handlers::handle_incoming_message;

pub async fn run_event_loop(bot: &mut Bot) {
    // W2 — replay chat threads paused behind an approval across the restart
    // (durable resume). Owned clones so the task is independent of the loop.
    {
        let deps = crate::tool_runner::ResumerDeps {
            store: bot.store.clone(),
            registry: bot.registry.clone(),
            bridge: bot.capability_bridge.clone(),
            sentinel: bot.sentinel.clone(),
            adapter: bot.ai_adapter.clone(),
            response_tx: bot.response_tx.clone(),
            policy: bot.config.tool_policy.clone(),
        };
        tokio::spawn(crate::tool_runner::resume_orphaned_loops(deps));
    }

    loop {
        tokio::select! {
            // Source 1: incoming chat messages from connectors.
            Some(msg) = bot.connector_rx.recv() => {
                if let Err(e) = handle_incoming_message(bot, &msg).await {
                    tracing::error!(
                        user_id = %msg.user_id,
                        error = %e,
                        "message processing failed — continuing"
                    );
                }
            }
            // Source 2: trigger events from the rule engine.
            Some(trigger) = bot.rule_rx.recv() => {
                if let Err(e) = handle_trigger_event(bot, &trigger).await {
                    tracing::error!(error = %e, "trigger processing failed — continuing");
                }
            }
            // Source 3: cadence ticks from the cooperation module (§5).
            result = bot.cadence_rx.recv() => match result {
                Ok(tick) => {
                    bot.cadence_tick
                        .store(tick.sequence.0, std::sync::atomic::Ordering::Relaxed);
                    handle_cadence_tick(bot, &tick).await;
                    // Strategic (colony) layer: review the whole colony every
                    // COLONY_INTERVAL ticks. Runs AFTER the per-formation tick so
                    // it never holds the formation lock during an LLM call.
                    if tick.sequence.0 % crate::colony::COLONY_INTERVAL == 0 {
                        crate::colony::commander::run(bot).await;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                    tracing::warn!(skipped = count, "cadence receiver lagged — skipping ticks");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::debug!("cadence channel closed");
                }
            },
            // Source 4: formation commands from runtime operations.
            Some(cmd) = bot.formation_cmd_rx.recv() => {
                tick_steps::handle_formation_command(bot, cmd).await;
            }
            // All channels closed — shutdown.
            else => {
                tracing::info!("all event channels closed — bot shutting down");
                break;
            }
        }
    }

    tick_steps::log_shutdown_snapshot(bot).await;
}

/// One pass over every viable formation. Per-formation logic is composed
/// in `tick_steps::run_tick`; this function owns lock acquisition and the
/// post-iteration tail cleanup.
async fn handle_cadence_tick(bot: &mut Bot, tick: &Tick) {
    let formation_count = bot.formations.read().await.len();
    if formation_count == 0 {
        return;
    }
    tracing::trace!(
        tick = tick.sequence.0,
        formations = formation_count,
        "cadence tick processing"
    );

    // Split-borrow Bot's fields explicitly so the per-formation iteration
    // and the deps both have valid lifetimes (Rust can't see through a
    // single `deps_from_bot(&mut bot)` while another field is also borrowed).
    let formations_arc = bot.formations.clone();
    let mut formations = formations_arc.write().await;
    let mut deps = tick_steps::TickDeps {
        bridge: &bot.capability_bridge,
        sentinel: &bot.sentinel,
        store: &bot.store,
        registry: &bot.registry,
        role_registry: &bot.role_registry,
        cadence: &bot.cadence,
        cadence_reports_rx: &mut bot.cadence_reports_rx,
        intervention_evaluator: &bot.intervention_evaluator,
        intervention_action: &bot.intervention_action,
        canvas_tx: bot.canvas_tx.as_ref(),
        cooperation_tx: bot.cooperation_tx.as_ref(),
    };

    for formation in formations.iter_mut() {
        if !formation.is_viable() || formation.paused {
            continue;
        }
        tick_steps::run_tick(formation, tick, &mut deps).await;
    }

    tick_steps::tail::reclaim_dead(&mut formations);
    tick_steps::tail::drain_member_subs(&mut formations);
    tick_steps::tail::drain_rally_events(&mut formations);
    tick_steps::tail::retain_viable(&mut formations);
}
