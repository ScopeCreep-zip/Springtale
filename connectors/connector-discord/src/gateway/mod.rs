use std::sync::Arc;

use twilight_gateway::{Event, EventTypeFlags, StreamExt as _};

/// Run the Discord gateway event loop.
///
/// Receives events from the Discord gateway shard and dispatches them
/// to the bot message pipeline via the dispatcher callback.
///
/// Twilight handles reconnection automatically inside `next_event()` —
/// no manual reconnect loop is needed (unlike IRC).
///
/// Interactions are deferred immediately (within 3 seconds) before
/// dispatching to the bot pipeline. This prevents Discord from showing
/// "This interaction failed" to the user.
///
/// Privacy: TYPING_START and PRESENCE_UPDATE events are silently discarded
/// via EventTypeFlags filtering — they never reach this code.
pub async fn gateway_loop(
    mut shard: twilight_gateway::Shard,
    http_client: Arc<twilight_http::Client>,
    application_id: u64,
    dispatcher: Arc<dyn Fn(serde_json::Value) + Send + Sync>,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) {
    tracing::info!("Discord gateway loop started");

    // Only receive events we care about — typing/presence never reach us.
    let wanted = EventTypeFlags::MESSAGE_CREATE
        | EventTypeFlags::INTERACTION_CREATE
        | EventTypeFlags::REACTION_ADD
        | EventTypeFlags::MEMBER_ADD;

    let app_id = twilight_model::id::Id::new(application_id);

    loop {
        tokio::select! {
            event = shard.next_event(wanted) => {
                match event {
                    Some(Ok(event)) => {
                        // For interactions, defer immediately before dispatching
                        if let Event::InteractionCreate(ref interaction) = event {
                            defer_interaction(&http_client, app_id, interaction).await;
                        }

                        if let Some(payload) = route_event(&event, application_id) {
                            dispatcher(payload);
                        }
                    }
                    Some(Err(e)) => {
                        tracing::warn!(kind = ?e.kind(), "Discord gateway error");
                    }
                    None => {
                        tracing::error!("Discord gateway stream ended unexpectedly");
                        break;
                    }
                }
            }
            _ = shutdown_rx.changed() => {
                tracing::info!("Discord gateway shutting down");
                break;
            }
        }
    }

    tracing::info!("Discord gateway loop exited");
}

/// Immediately defer an interaction so Discord shows "thinking..." instead of
/// "This interaction failed." The bot pipeline will send the actual response
/// as a regular message.
///
/// This MUST complete within 3 seconds of receiving the interaction.
async fn defer_interaction(
    http: &twilight_http::Client,
    app_id: twilight_model::id::Id<twilight_model::id::marker::ApplicationMarker>,
    interaction: &twilight_model::application::interaction::Interaction,
) {
    let response = twilight_model::http::interaction::InteractionResponse {
        kind: twilight_model::http::interaction::InteractionResponseType::DeferredChannelMessageWithSource,
        data: None,
    };

    if let Err(e) = http
        .interaction(app_id)
        .create_response(interaction.id, &interaction.token, &response)
        .await
    {
        tracing::warn!(
            interaction_id = %interaction.id,
            error = %e,
            "failed to defer interaction"
        );
    }
}

/// Route a gateway event to its trigger payload.
///
/// `app_id` is the bot's application id (== its user id for bots), used
/// to detect when the bot itself is @mentioned.
///
/// Returns None for events we don't route.
fn route_event(event: &Event, app_id: u64) -> Option<serde_json::Value> {
    match event {
        // All these variants are Box<T> in the Event enum
        Event::MessageCreate(msg) => Some(route_message_create(msg, app_id)),
        Event::InteractionCreate(interaction) => Some(route_interaction(interaction)),
        Event::ReactionAdd(reaction) => Some(route_reaction_add(reaction)),
        Event::MemberAdd(member) => Some(route_member_add(member)),
        _ => None,
    }
}

fn route_message_create(
    msg: &twilight_model::gateway::payload::incoming::MessageCreate,
    app_id: u64,
) -> serde_json::Value {
    // MessageCreate derefs to Message.
    // A message that @mentions the bot fires `app_mentioned` (the
    // discord-mention-ai-reply recipe's trigger); otherwise a guild
    // message is `message_received` and a DM is `dm_received`.
    let mentions_bot = msg.mentions.iter().any(|m| m.id.get() == app_id);
    let trigger = if mentions_bot {
        "app_mentioned"
    } else if msg.guild_id.is_some() {
        "message_received"
    } else {
        "dm_received"
    };

    serde_json::json!({
        "trigger": trigger,
        "message_id": msg.id.get().to_string(),
        "channel_id": msg.channel_id.get().to_string(),
        "guild_id": msg.guild_id.map(|g| g.get().to_string()),
        "user_id": msg.author.id.get().to_string(),
        "content": msg.content,
        "timestamp": msg.timestamp.as_secs(),
    })
}

fn route_interaction(
    interaction: &twilight_model::application::interaction::Interaction,
) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "trigger": "interaction_received",
        "interaction_id": interaction.id.get().to_string(),
        "interaction_token": interaction.token,
        "channel_id": interaction.channel.as_ref().map(|c| c.id.get().to_string()),
        "guild_id": interaction.guild_id.map(|g| g.get().to_string()),
    });

    // Extract user ID — from member (guild context) or user (DM context)
    if let Some(ref member) = interaction.member {
        if let Some(ref user) = member.user {
            payload["user_id"] = serde_json::json!(user.id.get().to_string());
        }
    } else if let Some(ref user) = interaction.user {
        payload["user_id"] = serde_json::json!(user.id.get().to_string());
    }

    // Extract command name and options if this is an application command
    if let Some(twilight_model::application::interaction::InteractionData::ApplicationCommand(
        ref cmd,
    )) = interaction.data
    {
        payload["command_name"] = serde_json::json!(cmd.name);

        // Flatten options into a simple key-value map
        let mut options = serde_json::Map::new();
        for opt in &cmd.options {
            let value = match &opt.value {
                twilight_model::application::interaction::application_command::CommandOptionValue::String(s) => {
                    serde_json::json!(s)
                }
                twilight_model::application::interaction::application_command::CommandOptionValue::Integer(i) => {
                    serde_json::json!(i)
                }
                twilight_model::application::interaction::application_command::CommandOptionValue::Boolean(b) => {
                    serde_json::json!(b)
                }
                twilight_model::application::interaction::application_command::CommandOptionValue::Number(n) => {
                    serde_json::json!(n)
                }
                other => serde_json::json!(format!("{other:?}")),
            };
            options.insert(opt.name.clone(), value);
        }
        if !options.is_empty() {
            payload["options"] = serde_json::Value::Object(options);
        }
    }

    payload
}

fn route_reaction_add(
    reaction: &twilight_model::gateway::payload::incoming::ReactionAdd,
) -> serde_json::Value {
    // ReactionAdd derefs to GatewayReaction
    let emoji = match &reaction.emoji {
        twilight_model::channel::message::EmojiReactionType::Custom { name, id, .. } => name
            .as_deref()
            .map(|n| format!("{n}:{}", id.get()))
            .unwrap_or_else(|| id.get().to_string()),
        twilight_model::channel::message::EmojiReactionType::Unicode { name } => name.clone(),
    };

    serde_json::json!({
        "trigger": "reaction_added",
        "user_id": reaction.user_id.get().to_string(),
        "channel_id": reaction.channel_id.get().to_string(),
        "message_id": reaction.message_id.get().to_string(),
        "guild_id": reaction.guild_id.map(|g| g.get().to_string()),
        "emoji": emoji,
    })
}

fn route_member_add(
    member: &twilight_model::gateway::payload::incoming::MemberAdd,
) -> serde_json::Value {
    // MemberAdd has guild_id directly, and derefs to Member (which has user, joined_at)
    serde_json::json!({
        "trigger": "member_joined",
        "user_id": member.user.id.get().to_string(),
        "guild_id": member.guild_id.get().to_string(),
        "joined_at": member.joined_at.map(|t| t.as_secs()),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_route_event_returns_none_for_unhandled() {
        let event = Event::GatewayHeartbeatAck;
        assert!(route_event(&event, 0).is_none());
    }
}
