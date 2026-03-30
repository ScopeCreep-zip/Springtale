use secrecy::SecretBox;
use serde::Deserialize;

/// Configuration for the Bluesky connector.
#[derive(Deserialize)]
pub struct BlueskyConfig {
    /// Bluesky handle or DID (e.g., "user.bsky.social" or "did:plc:...").
    pub identifier: String,

    /// Account password (app password recommended).
    #[serde(deserialize_with = "deserialize_secret")]
    pub password: SecretBox<String>,

    /// ATProto PDS base URL. Default: `https://bsky.social`.
    #[serde(default = "default_pds_base")]
    pub pds_base: String,

    /// Jetstream WebSocket URL for real-time events.
    /// Default: `wss://jetstream2.us-west.bsky.network/subscribe`
    #[serde(default = "default_jetstream_url")]
    pub jetstream_url: String,
}

fn default_pds_base() -> String {
    "https://bsky.social".to_owned()
}

fn default_jetstream_url() -> String {
    "wss://jetstream2.us-west.bsky.network/subscribe".to_owned()
}

fn deserialize_secret<'de, D>(deserializer: D) -> Result<SecretBox<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(SecretBox::new(Box::new(s)))
}

impl std::fmt::Debug for BlueskyConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlueskyConfig")
            .field("identifier", &self.identifier)
            .field("password", &"[REDACTED]")
            .field("pds_base", &self.pds_base)
            .field("jetstream_url", &self.jetstream_url)
            .finish()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_debug_redacts_password() {
        let config = BlueskyConfig {
            identifier: "test.bsky.social".to_owned(),
            password: SecretBox::new(Box::new("super_secret".to_owned())),
            pds_base: default_pds_base(),
            jetstream_url: default_jetstream_url(),
        };
        let debug = format!("{config:?}");
        assert!(!debug.contains("super_secret"));
        assert!(debug.contains("[REDACTED]"));
    }
}
