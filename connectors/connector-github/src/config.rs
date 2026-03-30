use secrecy::{ExposeSecret, SecretBox};
use serde::Deserialize;
use springtale_connector::config::{deserialize_secret, deserialize_secret_option};

/// Configuration for the GitHub connector.
///
/// Deserialized from TOML config. Credentials stored in `Secret<String>`.
#[derive(Deserialize)]
pub struct GithubConfig {
    /// GitHub Personal Access Token for API authentication.
    #[serde(deserialize_with = "deserialize_secret")]
    pub token: SecretBox<String>,

    /// Webhook secret for verifying incoming webhook payloads.
    /// Used for HMAC-SHA256 signature verification.
    #[serde(default, deserialize_with = "deserialize_secret_option")]
    pub webhook_secret: Option<SecretBox<String>>,

    /// GitHub API base URL. Default: `https://api.github.com`.
    #[serde(default = "default_api_base")]
    pub api_base: String,
}

fn default_api_base() -> String {
    "https://api.github.com".to_owned()
}

impl std::fmt::Debug for GithubConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GithubConfig")
            .field("token", &"[REDACTED]")
            .field("webhook_secret", &self.webhook_secret.as_ref().map(|_| "[REDACTED]"))
            .field("api_base", &self.api_base)
            .finish()
    }
}

impl GithubConfig {
    /// Clone the token into a new SecretBox for the client to hold.
    /// The token stays wrapped — only exposed at the HTTP call site.
    pub fn token_clone(&self) -> SecretBox<String> {
        // SECURITY: expose needed to clone into new SecretBox for client
        SecretBox::new(Box::new(self.token.expose_secret().clone()))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_debug_redacts_secrets() {
        let config = GithubConfig {
            token: SecretBox::new(Box::new("ghp_secret_token".to_owned())),
            webhook_secret: Some(SecretBox::new(Box::new("webhook_secret_value".to_owned()))),
            api_base: default_api_base(),
        };

        let debug = format!("{config:?}");
        assert!(!debug.contains("ghp_secret_token"));
        assert!(!debug.contains("webhook_secret_value"));
        assert!(debug.contains("[REDACTED]"));
    }

    #[test]
    fn test_token_clone_stays_wrapped() {
        let config = GithubConfig {
            token: SecretBox::new(Box::new("ghp_test".to_owned())),
            webhook_secret: None,
            api_base: default_api_base(),
        };

        let cloned = config.token_clone();
        assert_eq!(cloned.expose_secret(), "ghp_test");
    }
}
