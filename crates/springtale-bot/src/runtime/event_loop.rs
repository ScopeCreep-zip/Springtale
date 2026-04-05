use std::sync::Arc;

use springtale_connector::registry::store::ConnectorRegistry;
use springtale_core::rule::action::Action;
use tokio::sync::{RwLock, mpsc};

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
            match dispatch_bot_action(action, &bot.registry).await {
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

/// Dispatch a single rule action within the bot context.
///
/// Unlike the daemon's `dispatch_action` (which enqueues jobs), the bot
/// dispatches actions directly because it's interactive — users wait for
/// responses. Handles `RunConnector` through the capability-checked
/// registry API. Other action types log or pass through for Phase 1b.
///
/// Boxed return to support recursive `Chain` dispatch.
/// Starts at depth 0; `Chain` increments depth and checks
/// against `MAX_CHAIN_DEPTH` before recursing.
fn dispatch_bot_action<'a>(
    action: &'a Action,
    registry: &'a Arc<RwLock<ConnectorRegistry>>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send + 'a>> {
    dispatch_bot_action_with_depth(action, registry, 0)
}

fn dispatch_bot_action_with_depth<'a>(
    action: &'a Action,
    registry: &'a Arc<RwLock<ConnectorRegistry>>,
    depth: u32,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send + 'a>> {
    Box::pin(dispatch_bot_action_inner(action, registry, depth))
}

async fn dispatch_bot_action_inner(
    action: &Action,
    registry: &Arc<RwLock<ConnectorRegistry>>,
    depth: u32,
) -> Result<String, String> {
    match action {
        Action::RunConnector {
            connector,
            action: action_name,
            params,
        } => {
            let input = serde_json::Value::Object(params.clone());

            // Get Arc'd host + capability checker under lock, then drop
            // lock before the actual network call.
            let (host, checker) = {
                let reg = registry.read().await;
                reg.get_for_execute(connector).map_err(|e| e.to_string())?
            };

            match host.execute_checked(action_name, input, &checker).await {
                Ok(result) => {
                    tracing::info!(
                        connector = %connector,
                        action = %action_name,
                        success = result.success,
                        "bot: connector action executed"
                    );
                    Ok(result.message)
                }
                Err(e) => {
                    tracing::warn!(
                        connector = %connector,
                        action = %action_name,
                        error = %e,
                        "bot: connector action failed"
                    );
                    Err(e.to_string())
                }
            }
        }

        Action::SendMessage { text } => {
            // Phase 1b: log only. SendMessage from rules doesn't carry
            // destination context (user_id, channel_id). Cron/file watch
            // triggers have no chat context to route to.
            tracing::info!(text = %text, "bot: SendMessage (no destination context)");
            Ok(format!("message: {text}"))
        }

        Action::Delay { seconds } => {
            tokio::time::sleep(std::time::Duration::from_secs(*seconds)).await;
            tracing::debug!(seconds = seconds, "bot: delay completed");
            Ok(format!("delayed {seconds}s"))
        }

        Action::Notify { title, body } => {
            // Phase 1b: log only. Phase 2 adds notification channels.
            tracing::info!(title = %title, body = %body, "bot: NOTIFICATION");
            Ok(format!("notified: {title}"))
        }

        Action::WriteFile {
            destination,
            content,
            ..
        } => {
            const MAX_WRITE_FILE_BYTES: usize = 10 * 1024 * 1024;
            if content.len() > MAX_WRITE_FILE_BYTES {
                return Err(format!(
                    "file content size ({} bytes) exceeds maximum ({MAX_WRITE_FILE_BYTES} bytes)",
                    content.len()
                ));
            }
            tokio::fs::write(destination, content)
                .await
                .map_err(|e| format!("failed to write file {destination}: {e}"))?;
            tracing::info!(path = %destination, "bot: file written");
            Ok(format!("wrote {destination}"))
        }

        Action::RunShell { command } => {
            // Phase 1b: log only. ShellExec requires capability approval.
            tracing::info!(command = %command, "bot: SHELL (not executed — requires approval)");
            Ok(format!("shell logged: {command}"))
        }

        Action::Chain { steps } => {
            let new_depth = depth + 1;
            if new_depth > springtale_core::rule::action::MAX_CHAIN_DEPTH {
                return Err(format!(
                    "chain depth {new_depth} exceeds max {}",
                    springtale_core::rule::action::MAX_CHAIN_DEPTH
                ));
            }

            let mut results = Vec::new();
            for (i, step) in steps.iter().enumerate() {
                match dispatch_bot_action_with_depth(step, registry, new_depth).await {
                    Ok(msg) => results.push(msg),
                    Err(e) => {
                        tracing::warn!(step = i, error = %e, "bot: chain step failed");
                        return Err(format!("chain step {i} failed: {e}"));
                    }
                }
            }
            Ok(format!("chain completed: {} steps", results.len()))
        }

        Action::Transform { operation, .. } => {
            tracing::debug!(operation = %operation, "bot: transform pass-through");
            Ok(format!("transform: {operation}"))
        }

        Action::AiComplete { .. } => {
            tracing::debug!("bot: AI complete pass-through (NoopAdapter)");
            Ok("ai: noop".to_owned())
        }
    }
}

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

    // Phase 2a: tick processing is a placeholder that logs and updates momentum.
    // Full tick processing (collect TickReports from each member, detect
    // interference, trigger recovery) requires agent execution infrastructure
    // that will be wired when formations actually dispatch actions.
    drop(formations);

    let mut formations = bot.formations.write().await;
    for formation in formations.iter_mut() {
        if formation.is_viable() {
            // Record successful tick for momentum building
            formation.momentum.record_success();
        }
    }

    // Remove non-viable formations
    formations.retain(|f| f.is_viable());
}
