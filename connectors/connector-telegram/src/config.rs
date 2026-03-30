use secrecy::SecretBox;
use serde::Deserialize;
use springtale_connector::config::deserialize_secret;

/// Telegram connector configuration.
/// Bot token is wrapped in `Secret<String>` — never logged, zeroed on drop.
#[derive(Deserialize)]
pub struct TelegramConfig {
    /// Bot token from @BotFather (format: "123456:ABC-DEF...").
    #[serde(deserialize_with = "deserialize_secret")]
    pub bot_token: SecretBox<String>,

    /// Telegram Bot API base URL. Default: "https://api.telegram.org".
    #[serde(default = "default_api_base")]
    pub api_base: String,

    /// Update mode: "polling" or "webhook". Default: "polling".
    #[serde(default = "default_update_mode")]
    pub update_mode: String,

    /// Webhook callback URL (required when update_mode = "webhook").
    #[serde(default)]
    pub webhook_url: Option<String>,

    /// Long-polling timeout in seconds. Default: 30.
    #[serde(default = "default_poll_timeout")]
    pub poll_timeout: u64,
}

fn default_api_base() -> String {
    "https://api.telegram.org".to_owned()
}

fn default_update_mode() -> String {
    "polling".to_owned()
}

fn default_poll_timeout() -> u64 {
    30
}

impl std::fmt::Debug for TelegramConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TelegramConfig")
            .field("bot_token", &"[REDACTED]")
            .field("api_base", &self.api_base)
            .field("update_mode", &self.update_mode)
            .field("webhook_url", &self.webhook_url)
            .field("poll_timeout", &self.poll_timeout)
            .finish()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_debug_redacts_token() {
        let config = TelegramConfig {
            bot_token: SecretBox::new(Box::new("secret_token".to_owned())),
            api_base: default_api_base(),
            update_mode: default_update_mode(),
            webhook_url: None,
            poll_timeout: default_poll_timeout(),
        };
        let debug = format!("{config:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("secret_token"));
    }
}
