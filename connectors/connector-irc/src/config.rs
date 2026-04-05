use secrecy::SecretBox;
use serde::Deserialize;
use springtale_connector::config::deserialize_secret_option;

/// IRC connector configuration.
///
/// WARNING: IRC has NO end-to-end encryption. Server operators and
/// network observers can read all messages. Not recommended for
/// covert organizing. Use VPN/bouncer for IP privacy.
#[derive(Deserialize)]
pub struct IrcConfig {
    /// IRC server hostname (e.g., "irc.libera.chat").
    pub server: String,

    /// Server port. Default: 6697 (TLS).
    #[serde(default = "default_port")]
    pub port: u16,

    /// Use TLS. Default: true. MUST be true for production.
    #[serde(default = "default_use_tls")]
    pub use_tls: bool,

    /// Bot nickname.
    pub nick: String,

    /// NickServ password (optional). Secret<String>.
    #[serde(default, deserialize_with = "deserialize_secret_option")]
    pub nickserv_password: Option<SecretBox<String>>,

    /// Enable SASL PLAIN authentication (required by some networks like Libera.Chat).
    #[serde(default)]
    pub sasl_enabled: bool,

    /// Channels to auto-join on connect.
    #[serde(default)]
    pub channels: Vec<String>,

    /// Command prefix for bot commands. Default: "!".
    #[serde(default = "default_prefix")]
    pub command_prefix: String,

    /// Activity jitter in seconds (±0-N random delay on sends).
    /// Social graph protection per ARCHITECTURE.md §2.9.
    #[serde(default = "default_jitter")]
    pub message_jitter_secs: u64,
}

fn default_port() -> u16 {
    6697
}
fn default_use_tls() -> bool {
    true
}
fn default_prefix() -> String {
    "!".to_owned()
}
fn default_jitter() -> u64 {
    15
}

impl std::fmt::Debug for IrcConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IrcConfig")
            .field("server", &self.server)
            .field("port", &self.port)
            .field("use_tls", &self.use_tls)
            .field("nick", &self.nick)
            .field(
                "nickserv_password",
                &self.nickserv_password.as_ref().map(|_| "[REDACTED]"),
            )
            .field("channels", &self.channels)
            .field("command_prefix", &self.command_prefix)
            .field("message_jitter_secs", &self.message_jitter_secs)
            .finish()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_debug_redacts_password() {
        let config = IrcConfig {
            server: "irc.libera.chat".into(),
            port: default_port(),
            use_tls: true,
            nick: "bot".into(),
            nickserv_password: Some(SecretBox::new(Box::new("secret".into()))),
            sasl_enabled: false,
            channels: vec![],
            command_prefix: default_prefix(),
            message_jitter_secs: default_jitter(),
        };
        let debug = format!("{config:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("secret"));
    }
}
