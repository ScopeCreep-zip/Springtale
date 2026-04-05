use secrecy::SecretBox;
use serde::Deserialize;
use springtale_connector::config::deserialize_secret;

/// Nostr connector configuration.
///
/// CRITICAL: Nostr uses secp256k1 Schnorr signatures (BIP-340), NOT Ed25519.
/// The private key is stored separately from Springtale's Ed25519 identity.
#[derive(Deserialize)]
pub struct NostrConfig {
    /// Nostr private key (nsec bech32 or hex). secp256k1, NOT Ed25519.
    #[serde(deserialize_with = "deserialize_secret")]
    pub private_key: SecretBox<String>,

    /// Relay URLs to connect to (at least 1 required).
    pub relays: Vec<String>,

    /// DM encryption NIP: "nip44" (modern, default) or "nip04" (legacy, deprecated).
    #[serde(default = "default_dm_encryption")]
    pub dm_encryption: String,

    /// Activity jitter in seconds (±0-N random delay on sends).
    /// Social graph protection per ARCHITECTURE.md §2.9.
    #[serde(default = "default_jitter")]
    pub message_jitter_secs: u64,
}

fn default_dm_encryption() -> String {
    "nip44".to_owned()
}

fn default_jitter() -> u64 {
    30
}

impl std::fmt::Debug for NostrConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NostrConfig")
            .field("private_key", &"[REDACTED]")
            .field("relays", &self.relays)
            .field("dm_encryption", &self.dm_encryption)
            .field("message_jitter_secs", &self.message_jitter_secs)
            .finish()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_debug_redacts_key() {
        let config = NostrConfig {
            private_key: SecretBox::new(Box::new("nsec1secretkeyhere".to_owned())),
            relays: vec!["wss://relay.damus.io".to_owned()],
            dm_encryption: default_dm_encryption(),
            message_jitter_secs: default_jitter(),
        };
        let debug = format!("{config:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("nsec1"));
    }
}
