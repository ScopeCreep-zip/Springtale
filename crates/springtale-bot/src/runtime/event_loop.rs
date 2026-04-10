use tokio::sync::mpsc;

use crate::cooperation::cadence::Tick;
use crate::handler::registry::HandlerContext;
use crate::router::AliasResolver;
use crate::runtime::lifecycle::{Bot, OutgoingResponse};
use crate::state::session::SessionKey;

/// Send a response through the response channel, logging on failure.
async fn send_response(
    tx: &mpsc::Sender<OutgoingResponse>,
    channel_id: &str,
    text: String,
    connector: &str,
) {
    if let Err(e) = tx
        .send(OutgoingResponse {
            channel_id: channel_id.to_owned(),
            text,
            connector: connector.to_owned(),
        })
        .await
    {
        tracing::warn!(error = %e, "response channel closed — response dropped");
    }
}

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
            Ok(tick) = bot.cadence_rx.recv() => {
                handle_cadence_tick(bot, &tick).await;
            }

            // All channels closed — shutdown
            else => {
                tracing::info!("all event channels closed — bot shutting down");
                break;
            }
        }
    }
}

async fn handle_incoming_message(
    bot: &mut Bot,
    msg: &crate::runtime::lifecycle::IncomingMessage,
) -> Result<(), crate::error::BotError> {
    let session_key = SessionKey {
        user_id: msg.user_id.clone(),
        channel_id: msg.channel_id.clone(),
    };

    // Store in conversation context
    let _ = bot.context.push(&session_key, "user", &msg.text).await;

    // Route the message
    let route = bot.router.route(&msg.text, bot.config.persona.prefix);

    match route {
        crate::router::RouteResult::Command { name, args } => {
            if let Some(handler) = bot.handlers.get(&name) {
                let ctx = HandlerContext {
                    user_id: msg.user_id.clone(),
                    channel_id: msg.channel_id.clone(),
                    store: bot.store.clone(),
                    registry: bot.registry.clone(),
                    engine: bot.engine.clone(),
                };

                match handler.handle(&args, &ctx).await {
                    Ok(result) => {
                        // Reload aliases if the alias command was just executed
                        if name == "alias"
                            && let Ok(alias_pairs) = bot.store.list_aliases().await
                        {
                            let aliases = alias_pairs.into_iter().collect();
                            *bot.router.aliases_mut() = AliasResolver::new(aliases);
                        }

                        // Store bot response in context
                        let _ = bot
                            .context
                            .push(&session_key, "assistant", &result.response)
                            .await;

                        // Send response
                        send_response(
                            &bot.response_tx,
                            &msg.channel_id,
                            result.response,
                            &msg.source_connector,
                        )
                        .await;
                    }
                    Err(e) => {
                        tracing::error!(command = %name, error = %e, "handler error");
                        send_response(
                            &bot.response_tx,
                            &msg.channel_id,
                            format!("Error: {e}"),
                            &msg.source_connector,
                        )
                        .await;
                    }
                }
            } else {
                send_response(
                    &bot.response_tx,
                    &msg.channel_id,
                    format!("Command not found: {name}"),
                    &msg.source_connector,
                )
                .await;
            }
        }
        crate::router::RouteResult::NoMatch { suggestion } => {
            // Phase 2a: try AI fallback before static suggestion
            if let Some(response) = ai_fallback(bot, &session_key, &msg.text).await {
                let _ = bot.context.push(&session_key, "assistant", &response).await;
                send_response(
                    &bot.response_tx,
                    &msg.channel_id,
                    response,
                    &msg.source_connector,
                )
                .await;
            } else {
                send_response(
                    &bot.response_tx,
                    &msg.channel_id,
                    suggestion,
                    &msg.source_connector,
                )
                .await;
            }
        }
    }

    // Compact if needed
    let _ = bot.context.compact(&session_key).await;

    Ok(())
}

/// Try AI fallback for an unmatched message.
///
/// Returns `Some(response)` if AI is available and responds successfully.
/// Returns `None` if AI is unavailable, disabled, or errors — caller
/// should fall back to the static "Unknown command" suggestion.
async fn ai_fallback(
    bot: &mut Bot,
    session_key: &crate::state::session::SessionKey,
    user_text: &str,
) -> Option<String> {
    // Check if AI is available (NoopAdapter returns false → skip)
    if !bot.ai_adapter.is_available().await {
        return None;
    }

    // Gather recent conversation context
    let recent = bot.context.recent(session_key, 10).await.ok()?;

    // Build command list for the system prompt
    let commands: Vec<String> = bot
        .handlers
        .list_commands()
        .iter()
        .map(|(name, desc, _)| format!("/{name} — {desc}"))
        .collect();
    let command_list = commands.join("\n");

    // Build chat messages
    let mut messages = vec![springtale_ai::ChatMessage {
        role: "system".into(),
        content: format!(
            "You are {}, a helpful bot. The user sent a message that didn't match any command. \
             Respond conversationally. If they seem to want a command, suggest the right one.\n\n\
             Available commands:\n{}",
            bot.config.persona.name, command_list
        ),
    }];

    // Add conversation history
    for entry in recent.iter().rev() {
        messages.push(springtale_ai::ChatMessage {
            role: entry.author.clone(),
            content: String::from_utf8_lossy(&entry.content_encrypted).into_owned(),
        });
    }

    // Add the current user message
    messages.push(springtale_ai::ChatMessage {
        role: "user".into(),
        content: user_text.to_owned(),
    });

    let request = springtale_ai::AiRequest::Chat { messages };
    let options = springtale_ai::AiOptions {
        max_tokens: 512,
        timeout: std::time::Duration::from_secs(10),
        temperature: Some(0.7),
    };

    match bot.ai_adapter.complete(request, options).await {
        Ok(response) if !response.content.is_empty() => Some(response.content),
        Ok(_) => {
            tracing::debug!("AI fallback returned empty response");
            None
        }
        Err(springtale_ai::AiError::Disabled) => None, // Expected for NoopAdapter
        Err(e) => {
            tracing::warn!(error = %e, "AI fallback failed — using static suggestion");
            None
        }
    }
}

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
            match springtale_runtime::dispatch::dispatch_action(action, &bot.registry).await {
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
        let tier_json = serde_json::json!(format!("{:?}", formation.momentum.tier));
        if let Err(e) = bot
            .store
            .set_config(&key, &tier_json.to_string())
            .await
        {
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
