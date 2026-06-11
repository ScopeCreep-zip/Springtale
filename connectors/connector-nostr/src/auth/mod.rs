use nostr_sdk::prelude::*;

use crate::error::NostrError;

/// Parse a Nostr private key from an nsec bech32 string or hex.
///
/// CRITICAL: Nostr uses secp256k1 Schnorr (BIP-340), NOT Ed25519.
/// This key is completely separate from Springtale's identity system.
pub fn parse_keys(private_key: &secrecy::SecretBox<String>) -> Result<Keys, NostrError> {
    springtale_crypto::secret_use::with_str(private_key, |key_str| {
        Keys::parse(key_str).map_err(|e| NostrError::KeyError(format!("invalid nostr key: {e}")))
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_generated_key() {
        let keys = Keys::generate();
        let nsec = keys.secret_key().to_bech32().unwrap();
        let secret = secrecy::SecretBox::new(Box::new(nsec));
        let parsed = parse_keys(&secret).unwrap();
        assert_eq!(parsed.public_key(), keys.public_key());
    }

    #[test]
    fn test_parse_invalid_key() {
        let secret = secrecy::SecretBox::new(Box::new("not-a-key".to_owned()));
        assert!(parse_keys(&secret).is_err());
    }
}
