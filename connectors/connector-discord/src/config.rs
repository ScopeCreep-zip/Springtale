use secrecy::SecretBox;
use serde::Deserialize;
use springtale_connector::config::deserialize_secret;

/// Discord bot connector configuration.
///
/// WARNING: Discord complies with government data requests including
/// DHS subpoenas. Server admins can read ALL channels. IP addresses
/// are logged on every connection. Use a VPN.
///
/// Default: slash commands only (no MESSAGE_CONTENT intent).
/// Enable `enable_message_content` only if you understand the privacy cost.
#[derive(Deserialize)]
pub struct DiscordConfig {
    /// Bot token. Format: "NDcyNTk2MDcwMzU1MzE2NzQ2.D..."
    #[serde(deserialize_with = "deserialize_secret")]
    pub bot_token: SecretBox<String>,

    /// Application ID (for slash command registration).
    pub application_id: u64,

    /// Guild ID for guild-scoped slash commands (faster registration).
    /// If None, registers global commands (takes up to 1 hour to propagate).
    #[serde(default)]
    pub guild_id: Option<u64>,

    /// Whether to request MESSAGE_CONTENT privileged intent.
    /// WARNING: This lets the bot read ALL messages in ALL channels.
    /// Default: false (slash commands only — privacy-preferred).
    #[serde(default)]
    pub enable_message_content: bool,

    /// Whether to enable DM triggers.
    /// Default: false (reduces data exposure).
    #[serde(default)]
    pub enable_direct_messages: bool,

    /// Whether to enable reaction triggers.
    #[serde(default)]
    pub enable_reactions: bool,

    /// Publish-side jitter in seconds (0 = disabled).
    /// Delays outgoing messages by random 0..N seconds to obscure timing.
    #[serde(default)]
    pub message_jitter_secs: u64,

    /// Slash commands to register on startup.
    /// Each entry defines a command name and description.
    /// If empty, no commands are registered.
    #[serde(default)]
    pub commands: Vec<SlashCommandConfig>,
}

/// A slash command to register with Discord on startup.
#[derive(Deserialize, Clone, Debug)]
pub struct SlashCommandConfig {
    /// Command name (1-32 chars, lowercase alphanumeric + hyphens).
    pub name: String,
    /// Command description (1-100 chars).
    pub description: String,
}

impl std::fmt::Debug for DiscordConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiscordConfig")
            .field("bot_token", &"[REDACTED]")
            .field("application_id", &self.application_id)
            .field("guild_id", &self.guild_id)
            .field("enable_message_content", &self.enable_message_content)
            .field("enable_direct_messages", &self.enable_direct_messages)
            .field("enable_reactions", &self.enable_reactions)
            .field("message_jitter_secs", &self.message_jitter_secs)
            .finish()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_debug_redacts_bot_token() {
        let config: DiscordConfig = serde_json::from_value(serde_json::json!({
            "bot_token": "NDcyNTk2MDcwMzU1MzE2NzQ2.FAKE_TOKEN",
            "application_id": 123456789
        }))
        .unwrap();

        let debug_output = format!("{config:?}");
        assert!(debug_output.contains("[REDACTED]"));
        assert!(!debug_output.contains("FAKE_TOKEN"));
        assert!(!debug_output.contains("NDcy"));
    }

    #[test]
    fn test_config_defaults() {
        let config: DiscordConfig = serde_json::from_value(serde_json::json!({
            "bot_token": "test_token",
            "application_id": 1
        }))
        .unwrap();

        assert!(!config.enable_message_content);
        assert!(!config.enable_direct_messages);
        assert!(!config.enable_reactions);
        assert_eq!(config.message_jitter_secs, 0);
        assert!(config.guild_id.is_none());
    }
}
