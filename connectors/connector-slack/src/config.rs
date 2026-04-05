use secrecy::SecretBox;
use serde::Deserialize;
use springtale_connector::config::deserialize_secret;

/// Slack bot connector configuration.
///
/// WARNING: Slack is enterprise software. Workspace admins can read ALL
/// messages including DMs — with NO notification to users (changed 2018).
/// Enterprise Grid has full compliance export and eDiscovery.
/// Slack complies with government data requests.
/// Both tokens are revocable by workspace admins at any time.
///
/// Do NOT use Slack for covert organizing, asylum coordination, or
/// anything you wouldn't show your employer. Use Signal or Matrix instead.
#[derive(Deserialize)]
pub struct SlackConfig {
    /// Bot token (xoxb-...) for making API calls.
    #[serde(deserialize_with = "deserialize_secret")]
    pub bot_token: SecretBox<String>,

    /// App-level token (xapp-...) for Socket Mode WebSocket connection.
    /// Created in Slack app settings under Basic Information > App-Level Tokens.
    /// Requires `connections:write` scope.
    #[serde(deserialize_with = "deserialize_secret")]
    pub app_token: SecretBox<String>,

    /// Publish-side jitter in seconds (0 = disabled).
    /// Delays outgoing messages by random 0..N seconds to obscure timing.
    #[serde(default)]
    pub message_jitter_secs: u64,
}

impl std::fmt::Debug for SlackConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SlackConfig")
            .field("bot_token", &"[REDACTED]")
            .field("app_token", &"[REDACTED]")
            .field("message_jitter_secs", &self.message_jitter_secs)
            .finish()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_debug_redacts_tokens() {
        let config: SlackConfig = serde_json::from_value(serde_json::json!({
            "bot_token": "xoxb-fake-bot-token-12345",
            "app_token": "xapp-fake-app-token-67890"
        }))
        .unwrap();

        let debug_output = format!("{config:?}");
        assert!(debug_output.contains("[REDACTED]"));
        assert!(!debug_output.contains("xoxb"));
        assert!(!debug_output.contains("xapp"));
        assert!(!debug_output.contains("fake"));
    }

    #[test]
    fn test_config_defaults() {
        let config: SlackConfig = serde_json::from_value(serde_json::json!({
            "bot_token": "xoxb-test",
            "app_token": "xapp-test"
        }))
        .unwrap();

        assert_eq!(config.message_jitter_secs, 0);
    }
}
