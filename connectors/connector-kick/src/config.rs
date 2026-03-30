use secrecy::SecretBox;
use serde::Deserialize;
use springtale_connector::config::deserialize_secret;

/// Configuration for the Kick connector.
#[derive(Deserialize)]
pub struct KickConfig {
    /// OAuth 2.1 client ID.
    pub client_id: String,

    /// OAuth 2.1 client secret.
    #[serde(deserialize_with = "deserialize_secret")]
    pub client_secret: SecretBox<String>,

    /// OAuth redirect URI (must match Kick app settings).
    pub redirect_uri: String,

    /// Scopes to request during OAuth authorization.
    /// Default: user:read, channel:read, chat:write, events:subscribe
    #[serde(default = "default_scopes")]
    pub scopes: Vec<String>,

    /// Kick API base URL. Default: `https://api.kick.com`.
    #[serde(default = "default_api_base")]
    pub api_base: String,

    /// Kick OAuth server base URL. Default: `https://id.kick.com`.
    #[serde(default = "default_oauth_base")]
    pub oauth_base: String,

    /// Webhook callback URL that Kick sends events to.
    /// This must be a publicly accessible HTTPS URL.
    /// Set by springtaled based on its management API address.
    #[serde(default)]
    pub webhook_callback_url: Option<String>,
}

fn default_scopes() -> Vec<String> {
    vec![
        "user:read".to_owned(),
        "channel:read".to_owned(),
        "channel:write".to_owned(),
        "chat:write".to_owned(),
        "events:subscribe".to_owned(),
    ]
}

fn default_api_base() -> String {
    "https://api.kick.com".to_owned()
}

fn default_oauth_base() -> String {
    "https://id.kick.com".to_owned()
}

impl std::fmt::Debug for KickConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KickConfig")
            .field("client_id", &self.client_id)
            .field("client_secret", &"[REDACTED]")
            .field("redirect_uri", &self.redirect_uri)
            .field("scopes", &self.scopes)
            .field("api_base", &self.api_base)
            .field("oauth_base", &self.oauth_base)
            .finish()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_debug_redacts_secret() {
        let config = KickConfig {
            client_id: "test_id".to_owned(),
            client_secret: SecretBox::new(Box::new("super_secret".to_owned())),
            redirect_uri: "http://localhost:3000/callback".to_owned(),
            scopes: default_scopes(),
            api_base: default_api_base(),
            oauth_base: default_oauth_base(),
            webhook_callback_url: None,
        };
        let debug = format!("{config:?}");
        assert!(!debug.contains("super_secret"));
        assert!(debug.contains("[REDACTED]"));
    }

    #[test]
    fn test_default_scopes() {
        let scopes = default_scopes();
        assert_eq!(scopes.len(), 5);
        assert!(scopes.contains(&"channel:write".to_owned()));
        assert!(scopes.contains(&"chat:write".to_owned()));
    }
}
