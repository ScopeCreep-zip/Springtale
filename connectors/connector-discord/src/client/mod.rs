use async_trait::async_trait;

use crate::config::SlashCommandConfig;
use crate::error::DiscordError;

/// A guild text channel discovered via active enumeration.
///
/// Surfaces the minimum metadata needed to build a
/// `discord://guild/{guild_id}/channel/{channel_id}` workspace key.
#[derive(Debug, Clone)]
pub struct DiscoveredChannel {
    pub guild_id: u64,
    pub guild_name: String,
    pub channel_id: u64,
    pub channel_name: String,
}

/// Trait defining the Discord API surface used by actions.
/// Actions depend on this trait — enables mock testing.
#[async_trait]
pub trait DiscordApi: Send + Sync {
    /// Send a text message to a channel.
    async fn send_message(
        &self,
        channel_id: u64,
        content: &str,
    ) -> Result<serde_json::Value, DiscordError>;

    /// Send a rich embed to a channel.
    async fn send_embed(
        &self,
        channel_id: u64,
        embed: serde_json::Value,
    ) -> Result<serde_json::Value, DiscordError>;

    /// Edit an existing message.
    async fn edit_message(
        &self,
        channel_id: u64,
        message_id: u64,
        content: &str,
    ) -> Result<(), DiscordError>;

    /// Delete a message.
    async fn delete_message(&self, channel_id: u64, message_id: u64) -> Result<(), DiscordError>;

    /// Add a reaction to a message.
    async fn add_reaction(
        &self,
        channel_id: u64,
        message_id: u64,
        emoji: &str,
    ) -> Result<(), DiscordError>;

    /// Register slash commands with Discord.
    /// Guild-scoped if guild_id is Some (instant), global if None (up to 1hr propagation).
    async fn register_commands(
        &self,
        application_id: u64,
        guild_id: Option<u64>,
        commands: &[SlashCommandConfig],
    ) -> Result<(), DiscordError>;

    /// Enumerate every text channel the bot can see across every guild it
    /// is a member of.
    ///
    /// Walks `GET /users/@me/guilds` → `GET /guilds/{id}/channels` and
    /// filters down to `ChannelType::GuildText` only. Returns one entry
    /// per (guild, channel) pair.
    async fn list_destinations(&self) -> Result<Vec<DiscoveredChannel>, DiscordError>;
}

/// Concrete Discord client wrapping twilight-http.
///
/// Applies publish-side jitter before every outbound API call
/// to obscure activity timing from network observers.
pub struct DiscordClient {
    http: twilight_http::Client,
    jitter_secs: u64,
}

impl DiscordClient {
    /// Create a new DiscordClient.
    ///
    /// `token` is the raw bot token string (already exposed from Secret).
    pub fn new(token: String, jitter_secs: u64) -> Self {
        let http = twilight_http::Client::new(token);
        Self { http, jitter_secs }
    }

    /// Access the underlying twilight HTTP client (for slash command registration, etc.).
    pub fn http(&self) -> &twilight_http::Client {
        &self.http
    }

    /// Apply publish-side jitter before sending.
    async fn apply_jitter(&self) {
        if self.jitter_secs > 0 {
            let jitter = rand::random::<u64>() % self.jitter_secs;
            tokio::time::sleep(std::time::Duration::from_secs(jitter)).await;
        }
    }
}

#[async_trait]
impl DiscordApi for DiscordClient {
    async fn send_message(
        &self,
        channel_id: u64,
        content: &str,
    ) -> Result<serde_json::Value, DiscordError> {
        self.apply_jitter().await;
        let channel = twilight_model::id::Id::new(channel_id);

        // Validation errors are deferred and surface on .await
        let response = self
            .http
            .create_message(channel)
            .content(content)
            .await
            .map_err(|e| DiscordError::SendFailed(format!("create_message failed: {e}")))?;

        let msg = response
            .model()
            .await
            .map_err(|e| DiscordError::ApiError(format!("failed to deserialize message: {e}")))?;

        Ok(serde_json::json!({
            "id": msg.id.get(),
            "channel_id": msg.channel_id.get(),
            "content": msg.content,
        }))
    }

    async fn send_embed(
        &self,
        channel_id: u64,
        embed: serde_json::Value,
    ) -> Result<serde_json::Value, DiscordError> {
        self.apply_jitter().await;
        let channel = twilight_model::id::Id::new(channel_id);

        // Build embed from JSON input
        let title = embed.get("title").and_then(|t| t.as_str()).unwrap_or("");
        let description = embed.get("description").and_then(|d| d.as_str());
        let color = embed.get("color").and_then(|c| c.as_u64()).unwrap_or(0) as u32;

        let mut embed_obj = twilight_model::channel::message::embed::Embed {
            author: None,
            color: Some(color),
            description: description.map(|d| d.to_owned()),
            fields: Vec::new(),
            footer: None,
            image: None,
            kind: "rich".to_owned(),
            provider: None,
            thumbnail: None,
            timestamp: None,
            title: Some(title.to_owned()),
            url: None,
            video: None,
        };

        // Add fields if provided
        if let Some(fields) = embed.get("fields").and_then(|f| f.as_array()) {
            for field in fields {
                let name = field
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_owned();
                let value = field
                    .get("value")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned();
                let inline = field
                    .get("inline")
                    .and_then(|i| i.as_bool())
                    .unwrap_or(false);
                embed_obj
                    .fields
                    .push(twilight_model::channel::message::embed::EmbedField {
                        inline,
                        name,
                        value,
                    });
            }
        }

        let response = self
            .http
            .create_message(channel)
            .embeds(&[embed_obj])
            .await
            .map_err(|e| DiscordError::SendFailed(format!("create_message (embed) failed: {e}")))?;

        let msg = response
            .model()
            .await
            .map_err(|e| DiscordError::ApiError(format!("failed to deserialize message: {e}")))?;

        Ok(serde_json::json!({
            "id": msg.id.get(),
            "channel_id": msg.channel_id.get(),
        }))
    }

    async fn edit_message(
        &self,
        channel_id: u64,
        message_id: u64,
        content: &str,
    ) -> Result<(), DiscordError> {
        self.apply_jitter().await;
        let channel = twilight_model::id::Id::new(channel_id);
        let message = twilight_model::id::Id::new(message_id);

        self.http
            .update_message(channel, message)
            .content(Some(content))
            .await
            .map_err(|e| DiscordError::SendFailed(format!("update_message failed: {e}")))?;

        Ok(())
    }

    async fn delete_message(&self, channel_id: u64, message_id: u64) -> Result<(), DiscordError> {
        self.apply_jitter().await;
        let channel = twilight_model::id::Id::new(channel_id);
        let message = twilight_model::id::Id::new(message_id);

        self.http
            .delete_message(channel, message)
            .await
            .map_err(|e| DiscordError::SendFailed(format!("delete_message failed: {e}")))?;

        Ok(())
    }

    async fn add_reaction(
        &self,
        channel_id: u64,
        message_id: u64,
        emoji: &str,
    ) -> Result<(), DiscordError> {
        self.apply_jitter().await;
        let channel = twilight_model::id::Id::new(channel_id);
        let message = twilight_model::id::Id::new(message_id);

        // Unicode emoji string
        let request_emoji =
            twilight_http::request::channel::reaction::RequestReactionType::Unicode { name: emoji };

        self.http
            .create_reaction(channel, message, &request_emoji)
            .await
            .map_err(|e| DiscordError::SendFailed(format!("create_reaction failed: {e}")))?;

        Ok(())
    }

    async fn list_destinations(&self) -> Result<Vec<DiscoveredChannel>, DiscordError> {
        let guilds_resp = self
            .http
            .current_user_guilds()
            .await
            .map_err(|e| DiscordError::ApiError(format!("current_user_guilds failed: {e}")))?;
        let guilds = guilds_resp
            .models()
            .await
            .map_err(|e| DiscordError::ApiError(format!("guilds.models failed: {e}")))?;

        let mut out = Vec::new();
        for g in guilds {
            let g_id = g.id.get();
            let g_name = g.name.clone();

            let channels_resp = self.http.guild_channels(g.id).await.map_err(|e| {
                DiscordError::ApiError(format!(
                    "guild_channels failed (guild {g_id}): {e}"
                ))
            })?;
            let channels = channels_resp.models().await.map_err(|e| {
                DiscordError::ApiError(format!(
                    "channels.models failed (guild {g_id}): {e}"
                ))
            })?;

            for c in channels {
                if c.kind != twilight_model::channel::ChannelType::GuildText {
                    continue;
                }
                let c_name = c.name.clone().unwrap_or_default();
                out.push(DiscoveredChannel {
                    guild_id: g_id,
                    guild_name: g_name.clone(),
                    channel_id: c.id.get(),
                    channel_name: c_name,
                });
            }
        }
        Ok(out)
    }

    async fn register_commands(
        &self,
        application_id: u64,
        guild_id: Option<u64>,
        commands: &[SlashCommandConfig],
    ) -> Result<(), DiscordError> {
        let app_id = twilight_model::id::Id::new(application_id);

        // Build Command structs from config
        let tw_commands: Vec<twilight_model::application::command::Command> = commands
            .iter()
            .map(|cmd| twilight_model::application::command::Command {
                application_id: Some(app_id),
                contexts: None,
                default_member_permissions: None,
                #[allow(deprecated)]
                dm_permission: None,
                description: cmd.description.clone(),
                description_localizations: None,
                guild_id: None,
                id: None,
                integration_types: None,
                kind: twilight_model::application::command::CommandType::ChatInput,
                name: cmd.name.clone(),
                name_localizations: None,
                nsfw: None,
                options: Vec::new(),
                version: twilight_model::id::Id::new(1), // Discord sets this on response
            })
            .collect();

        if let Some(gid) = guild_id {
            // Guild-scoped: instant registration
            let guild = twilight_model::id::Id::new(gid);
            self.http
                .interaction(app_id)
                .set_guild_commands(guild, &tw_commands)
                .await
                .map_err(|e| {
                    DiscordError::ApiError(format!("failed to register guild commands: {e}"))
                })?;
        } else {
            // Global: up to 1 hour propagation
            self.http
                .interaction(app_id)
                .set_global_commands(&tw_commands)
                .await
                .map_err(|e| {
                    DiscordError::ApiError(format!("failed to register global commands: {e}"))
                })?;
        }

        Ok(())
    }
}

#[cfg(test)]
pub mod test_helpers {
    use super::*;

    pub struct MockDiscordApi;

    #[async_trait]
    impl DiscordApi for MockDiscordApi {
        async fn send_message(
            &self,
            channel_id: u64,
            content: &str,
        ) -> Result<serde_json::Value, DiscordError> {
            Ok(serde_json::json!({
                "id": 123456789,
                "channel_id": channel_id,
                "content": content,
            }))
        }

        async fn send_embed(
            &self,
            channel_id: u64,
            _embed: serde_json::Value,
        ) -> Result<serde_json::Value, DiscordError> {
            Ok(serde_json::json!({
                "id": 123456789,
                "channel_id": channel_id,
            }))
        }

        async fn edit_message(
            &self,
            _channel_id: u64,
            _message_id: u64,
            _content: &str,
        ) -> Result<(), DiscordError> {
            Ok(())
        }

        async fn delete_message(
            &self,
            _channel_id: u64,
            _message_id: u64,
        ) -> Result<(), DiscordError> {
            Ok(())
        }

        async fn add_reaction(
            &self,
            _channel_id: u64,
            _message_id: u64,
            _emoji: &str,
        ) -> Result<(), DiscordError> {
            Ok(())
        }

        async fn register_commands(
            &self,
            _application_id: u64,
            _guild_id: Option<u64>,
            _commands: &[SlashCommandConfig],
        ) -> Result<(), DiscordError> {
            Ok(())
        }

        async fn list_destinations(&self) -> Result<Vec<DiscoveredChannel>, DiscordError> {
            // 2 guilds × 3 text channels each = 6 rows, matches the test plan.
            let mut out = Vec::new();
            for g in 0..2u64 {
                let guild_id = 1000 + g;
                let guild_name = format!("Guild {g}");
                for c in 0..3u64 {
                    out.push(DiscoveredChannel {
                        guild_id,
                        guild_name: guild_name.clone(),
                        channel_id: guild_id * 10 + c,
                        channel_name: format!("channel-{c}"),
                    });
                }
            }
            Ok(out)
        }
    }
}
