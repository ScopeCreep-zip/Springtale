use secrecy::SecretBox;
use serde::Deserialize;
use springtale_connector::config::deserialize_secret_option;

/// Default `opencode serve` address — loopback, the documented default port.
const DEFAULT_BASE_URL: &str = "http://127.0.0.1:4096";
/// OpenCode's basic-auth username is fixed; only the password varies.
pub const OPENCODE_USERNAME: &str = "opencode";

/// Configuration for the OpenCode connector.
///
/// Deserialized from TOML config. Never serialized. The `password`
/// (matching the daemon's `OPENCODE_SERVER_PASSWORD`) is the only secret.
#[derive(Deserialize)]
pub struct OpenCodeConfig {
    /// Base URL of the running `opencode serve` daemon.
    /// Default: `http://127.0.0.1:4096`.
    #[serde(default = "default_base_url")]
    pub base_url: String,

    /// Daemon HTTP basic-auth password (the daemon's
    /// `OPENCODE_SERVER_PASSWORD`). Omit when the daemon runs without auth.
    #[serde(default, deserialize_with = "deserialize_secret_option")]
    pub password: Option<SecretBox<String>>,

    /// Optional model id to pass through on each prompt
    /// (e.g. `anthropic/claude-sonnet-4`). When `None`, the daemon's
    /// configured default model is used.
    #[serde(default)]
    pub model: Option<String>,

    /// Optional agent name to route prompts to a specific opencode agent.
    #[serde(default)]
    pub agent: Option<String>,
}

fn default_base_url() -> String {
    DEFAULT_BASE_URL.to_owned()
}

impl std::fmt::Debug for OpenCodeConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenCodeConfig")
            .field("base_url", &self.base_url)
            .field("password", &self.password.as_ref().map(|_| "[REDACTED]"))
            .field("model", &self.model)
            .field("agent", &self.agent)
            .finish()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_loopback_4096() {
        let config: OpenCodeConfig = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(config.base_url, "http://127.0.0.1:4096");
        assert!(config.password.is_none());
        assert!(config.model.is_none());
    }

    #[test]
    fn debug_redacts_password() {
        let config: OpenCodeConfig =
            serde_json::from_value(serde_json::json!({ "password": "hunter2" })).unwrap();
        let rendered = format!("{config:?}");
        assert!(rendered.contains("[REDACTED]"));
        assert!(!rendered.contains("hunter2"));
    }
}
