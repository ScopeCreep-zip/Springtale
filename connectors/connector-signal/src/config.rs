use serde::Deserialize;

/// Signal connector configuration.
///
/// Bridges to a signal-cli daemon running separately. The daemon handles
/// all Signal Protocol operations (encryption, key exchange, registration).
/// Springtale communicates via HTTP JSON-RPC to the daemon.
///
/// PRIVACY: The phone number is stored in signal-cli's local data
/// (~/.local/share/signal-cli/data/), NOT in this config. Springtale
/// only stores the daemon URL and account identifier.
///
/// WARNING: signal-cli stores message database and encryption keys in
/// plaintext on local disk. For device seizure protection, use full-disk
/// encryption and Springtale's --ephemeral mode.
#[derive(Deserialize)]
pub struct SignalConfig {
    /// signal-cli daemon HTTP endpoint (e.g., "http://localhost:8080").
    /// The daemon must be started separately:
    /// `signal-cli -a +NUMBER daemon --http localhost:8080`
    pub daemon_url: String,

    /// Account identifier — NOT the phone number.
    /// Used to identify which account to use in multi-account daemon mode.
    /// Can be a UUID or any identifier the user chooses.
    pub account_id: String,

    /// Publish-side jitter in seconds (0 = disabled).
    #[serde(default)]
    pub message_jitter_secs: u64,
}

impl std::fmt::Debug for SignalConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignalConfig")
            .field("daemon_url", &self.daemon_url)
            .field("account_id", &self.account_id)
            .field("message_jitter_secs", &self.message_jitter_secs)
            .finish()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_config_deserialize() {
        let config: SignalConfig = serde_json::from_value(serde_json::json!({
            "daemon_url": "http://localhost:8080",
            "account_id": "main"
        }))
        .unwrap();

        assert_eq!(config.daemon_url, "http://localhost:8080");
        assert_eq!(config.message_jitter_secs, 0);
    }

    #[test]
    fn test_debug_no_sensitive_data() {
        let config: SignalConfig = serde_json::from_value(serde_json::json!({
            "daemon_url": "http://localhost:8080",
            "account_id": "main"
        }))
        .unwrap();

        let debug = format!("{config:?}");
        // No phone numbers should appear
        assert!(!debug.contains("+1"));
    }
}
