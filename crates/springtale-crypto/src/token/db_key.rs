//! Database encryption key derivation from the vault passphrase.
//!
//! Uses HMAC-SHA256 with a dedicated context string so the DB key is
//! deterministically derived from the passphrase, but distinct from the
//! API token hash (which uses `"springtale-api-token"` as its context).
//!
//! Keeping both derivations in this crate means `springtaled` boot and
//! `springtale init` compute the same key without copy-pasting the HMAC
//! wiring (and getting the key/msg order wrong — see the historical bug
//! in the CLI's `trace` command).

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Domain-separation context. Bump the suffix when the derivation scheme
/// changes so old databases can be detected and migrated.
const DB_KEY_CONTEXT: &[u8] = b"springtale-db-encryption-v1";

/// Derive a 32-byte database encryption key from the vault passphrase.
///
/// Infallible: HMAC-SHA256 accepts any key size.
pub fn derive_db_encryption_key(passphrase: &[u8]) -> [u8; 32] {
    #[allow(clippy::expect_used)]
    let mut mac =
        HmacSha256::new_from_slice(passphrase).expect("HMAC-SHA256 accepts any key size");
    mac.update(DB_KEY_CONTEXT);
    let bytes = mac.finalize().into_bytes();
    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes);
    key
}

/// Hex-encoded form for passing to SQLite's `PRAGMA key`.
pub fn derive_db_encryption_key_hex(passphrase: &[u8]) -> String {
    hex::encode(derive_db_encryption_key(passphrase))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn derivation_is_deterministic() {
        let a = derive_db_encryption_key(b"correct horse battery staple");
        let b = derive_db_encryption_key(b"correct horse battery staple");
        assert_eq!(a, b);
    }

    #[test]
    fn different_passphrases_produce_different_keys() {
        let a = derive_db_encryption_key(b"one");
        let b = derive_db_encryption_key(b"two");
        assert_ne!(a, b);
    }

    #[test]
    fn db_key_differs_from_api_token_for_same_passphrase() {
        // Domain separation must prevent the same passphrase from producing
        // the same material for different purposes.
        let passphrase = b"shared";
        let db = derive_db_encryption_key(passphrase);
        let api = crate::token::api_token::derive_api_token_hash(passphrase);
        assert_ne!(db, api);
    }

    #[test]
    fn hex_form_is_64_chars() {
        let hex = derive_db_encryption_key_hex(b"x");
        assert_eq!(hex.len(), 64);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
