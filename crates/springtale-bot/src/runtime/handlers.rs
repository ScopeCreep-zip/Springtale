//! Message handlers — incoming message routing, AI fallback, response sending.
//!
//! Extracted from event_loop.rs for single-concern modules.

use tokio::sync::mpsc;

use crate::handler::registry::HandlerContext;
use crate::router::AliasResolver;
use crate::runtime::lifecycle::{Bot, OutgoingResponse};
use crate::state::session::SessionKey;

/// Send a response through the response channel, logging on failure.
pub(super) async fn send_response(
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

/// Check if a user is allowed to interact with the bot.
///
/// Access policy (matches OpenClaw's DM pairing model):
/// - Owner (bot:owner_id config key) is always allowed
/// - Paired users (paired:{user_id} config key) are allowed
/// - If no owner is set, the first user to message becomes the owner
/// - Unknown users receive a pairing code they must give to the owner
async fn check_access(
    bot: &mut Bot,
    msg: &crate::runtime::lifecycle::IncomingMessage,
) -> Result<bool, crate::error::BotError> {
    let user_id = &msg.user_id;

    // Check if pairing is disabled (open access mode)
    if let Ok(Some(mode)) = bot.store.get_config("bot:access_mode").await
        && mode.trim_matches('"') == "open"
    {
        return Ok(true);
    }

    // Check if this user is the owner
    match bot.store.get_config("bot:owner_id").await {
        Ok(Some(owner)) => {
            let owner = owner.trim_matches('"');
            if owner == user_id {
                return Ok(true);
            }
        }
        Ok(None) => {
            // No owner set — first user becomes owner (TOFU)
            let owner_val = format!("\"{user_id}\"");
            if let Err(e) = bot.store.set_config("bot:owner_id", &owner_val).await {
                tracing::error!(error = %e, "failed to persist bot owner — denying access");
                return Ok(false);
            }
            tracing::info!(user_id = %user_id, "first user registered as bot owner");
            return Ok(true);
        }
        Err(e) => {
            // Database error — deny access rather than accidentally granting ownership
            tracing::error!(error = %e, "failed to check bot owner — denying access");
            return Ok(false);
        }
    }

    // Check if user is paired
    let paired_key = format!("paired:{user_id}");
    if let Ok(Some(_)) = bot.store.get_config(&paired_key).await {
        return Ok(true);
    }

    // Rate limit: check if a code was recently generated for this user
    let rate_key = format!("pairing_rate:{user_id}");
    if let Ok(Some(last)) = bot.store.get_config(&rate_key).await
        && let Ok(last_time) = chrono::DateTime::parse_from_rfc3339(last.trim_matches('"'))
    {
        let elapsed = chrono::Utc::now() - last_time.with_timezone(&chrono::Utc);
        if elapsed.num_minutes() < 30 {
            send_response(
                &bot.response_tx,
                &msg.channel_id,
                "A pairing code was already generated recently. Please wait or ask the bot owner to approve it.".into(),
                &msg.source_connector,
            )
            .await;
            return Ok(false);
        }
    }

    // Generate pairing code and store with timestamp
    let code = generate_pairing_code();
    let code_key = format!("pairing_code:{code}");
    let now = chrono::Utc::now();
    let code_val = serde_json::json!({
        "user_id": user_id,
        "channel_id": msg.channel_id,
        "connector": msg.source_connector,
        "created_at": now.to_rfc3339(),
    })
    .to_string();
    if let Err(e) = bot.store.set_config(&code_key, &code_val).await {
        tracing::error!(error = %e, "failed to store pairing code");
        return Ok(false);
    }

    // Record rate limit timestamp
    let rate_val = format!("\"{}\"", now.to_rfc3339());
    if let Err(e) = bot.store.set_config(&rate_key, &rate_val).await {
        tracing::warn!(error = %e, "failed to store rate limit — continuing");
    }

    send_response(
        &bot.response_tx,
        &msg.channel_id,
        format!(
            "Access requires pairing. Give this code to the bot owner:\n\n{code}\n\n\
             Owner: send /pair approve {code}\n\
             Code expires in 60 minutes."
        ),
        &msg.source_connector,
    )
    .await;

    Ok(false)
}

/// Generate an 8-character pairing code from an unambiguous alphabet.
///
/// Uses 32-char alphabet (A-Z minus O/I, 0-9 minus 0/1) for ~40 bits entropy.
/// Avoids characters that are easily confused when read aloud or handwritten.
fn generate_pairing_code() -> String {
    use rand::RngCore;
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut bytes = [0u8; 8];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes
        .iter()
        .map(|b| ALPHABET[(*b as usize) % ALPHABET.len()] as char)
        .collect()
}

/// Handle an incoming chat message — route to command handler or AI fallback.
pub(super) async fn handle_incoming_message(
    bot: &mut Bot,
    msg: &crate::runtime::lifecycle::IncomingMessage,
) -> Result<(), crate::error::BotError> {
    // Access control — check pairing before processing
    if !check_access(bot, msg).await? {
        return Ok(());
    }

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
    let mut messages = vec![springtale_ai::ChatMessage::text(
        "system",
        format!(
            "You are {}, a helpful bot. The user sent a message that didn't match any command. \
             Respond conversationally. If they seem to want a command, suggest the right one. \
             You may also call connector tools to take actions on the user's behalf.\n\n\
             Available commands:\n{}",
            bot.config.persona.name, command_list
        ),
    )];

    // Add conversation history
    for entry in recent.iter().rev() {
        messages.push(springtale_ai::ChatMessage::text(
            entry.author.clone(),
            String::from_utf8_lossy(&entry.content_encrypted).into_owned(),
        ));
    }

    // Add the current user message
    messages.push(springtale_ai::ChatMessage::text("user", user_text.to_owned()));

    let options = springtale_ai::AiOptions {
        max_tokens: 512,
        timeout: std::time::Duration::from_secs(30),
        temperature: Some(0.7),
    };

    // Use the tool-calling runner so the model can invoke any enabled
    // connector action (including cross-channel messaging) under the
    // existing capability sandbox. `tool_runner::run_with_tools` does
    // the full loop: complete → execute tools → feed results → repeat
    // until the model stops asking for tools or the iteration cap hits.
    match crate::tool_runner::run_with_tools(
        bot.ai_adapter.as_ref(),
        &bot.registry,
        messages,
        options,
    )
    .await
    {
        Ok(response) if !response.content.is_empty() => Some(response.content),
        Ok(_) => {
            tracing::debug!("AI fallback returned empty response");
            None
        }
        Err(crate::tool_runner::ToolRunnerError::Ai(springtale_ai::AiError::Disabled)) => None,
        Err(e) => {
            tracing::warn!(error = %e, "AI fallback failed — using static suggestion");
            None
        }
    }
}
