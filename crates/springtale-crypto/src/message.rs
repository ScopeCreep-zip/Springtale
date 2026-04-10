//! Message-level encryption — XChaCha20-Poly1305 for individual messages.
//!
//! Used by bot conversation memory to encrypt each message at rest.
//! Each message gets its own random 24-byte nonce (generated at write time
//! in springtale-bot/memory/context.rs). The key is derived from the vault
//! at bot initialization and held in memory while the bot is running.
//!
//! XChaCha20-Poly1305 chosen because:
//! - Same algorithm as the vault (consistency)
//! - 24-byte nonce eliminates collision risk with random nonces
//! - AEAD provides both confidentiality and integrity
//! - Pure Rust (chacha20poly1305 crate), no native deps

use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit},
};

use crate::error::CryptoError;

/// Encrypt a message with XChaCha20-Poly1305.
///
/// # Arguments
/// - `plaintext` — the message content bytes
/// - `nonce` — 24-byte random nonce (unique per message)
/// - `key` — 32-byte encryption key (derived from vault)
///
/// # Returns
/// Ciphertext with 16-byte Poly1305 authentication tag appended.
pub fn encrypt_message(
    plaintext: &[u8],
    nonce: &[u8; 24],
    key: &[u8; 32],
) -> Result<Vec<u8>, CryptoError> {
    let cipher = XChaCha20Poly1305::new_from_slice(key)
        .map_err(|e| CryptoError::EncryptionFailed(format!("invalid key: {e}")))?;
    let xnonce = XNonce::from_slice(nonce);
    cipher
        .encrypt(xnonce, plaintext)
        .map_err(|e| CryptoError::EncryptionFailed(format!("encrypt failed: {e}")))
}

/// Decrypt a message with XChaCha20-Poly1305.
///
/// # Arguments
/// - `ciphertext` — encrypted bytes (includes 16-byte auth tag)
/// - `nonce` — 24-byte nonce used during encryption
/// - `key` — 32-byte encryption key
///
/// # Returns
/// Decrypted plaintext bytes.
pub fn decrypt_message(
    ciphertext: &[u8],
    nonce: &[u8; 24],
    key: &[u8; 32],
) -> Result<Vec<u8>, CryptoError> {
    let cipher = XChaCha20Poly1305::new_from_slice(key)
        .map_err(|e| CryptoError::DecryptionFailed(format!("invalid key: {e}")))?;
    let xnonce = XNonce::from_slice(nonce);
    cipher
        .decrypt(xnonce, ciphertext)
        .map_err(|e| CryptoError::DecryptionFailed(format!("decrypt failed: {e}")))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn test_key() -> [u8; 32] {
        [42u8; 32]
    }

    fn test_nonce() -> [u8; 24] {
        [7u8; 24]
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = test_key();
        let nonce = test_nonce();
        let plaintext = b"Hello from the springtale bot memory";

        let ciphertext = encrypt_message(plaintext, &nonce, &key).unwrap();
        assert_ne!(&ciphertext, plaintext, "ciphertext must differ from plaintext");

        let decrypted = decrypt_message(&ciphertext, &nonce, &key).unwrap();
        assert_eq!(&decrypted, plaintext);
    }

    #[test]
    fn test_wrong_key_fails() {
        let key = test_key();
        let nonce = test_nonce();
        let ciphertext = encrypt_message(b"secret", &nonce, &key).unwrap();

        let wrong_key = [99u8; 32];
        let result = decrypt_message(&ciphertext, &nonce, &wrong_key);
        assert!(result.is_err());
    }

    #[test]
    fn test_wrong_nonce_fails() {
        let key = test_key();
        let nonce = test_nonce();
        let ciphertext = encrypt_message(b"secret", &nonce, &key).unwrap();

        let wrong_nonce = [99u8; 24];
        let result = decrypt_message(&ciphertext, &wrong_nonce, &key);
        assert!(result.is_err());
    }

    #[test]
    fn test_tampered_ciphertext_fails() {
        let key = test_key();
        let nonce = test_nonce();
        let mut ciphertext = encrypt_message(b"secret", &nonce, &key).unwrap();

        // Flip a bit
        if let Some(byte) = ciphertext.first_mut() {
            *byte ^= 0x01;
        }

        let result = decrypt_message(&ciphertext, &nonce, &key);
        assert!(result.is_err(), "tampered ciphertext must fail authentication");
    }

    #[test]
    fn test_empty_plaintext() {
        let key = test_key();
        let nonce = test_nonce();
        let ciphertext = encrypt_message(b"", &nonce, &key).unwrap();
        // Even empty plaintext produces ciphertext (16-byte auth tag)
        assert_eq!(ciphertext.len(), 16);
        let decrypted = decrypt_message(&ciphertext, &nonce, &key).unwrap();
        assert!(decrypted.is_empty());
    }
}
