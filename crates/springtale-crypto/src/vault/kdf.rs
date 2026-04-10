use argon2::{Algorithm, Argon2, Params, Version};
use secrecy::SecretBox;
use zeroize::Zeroize;

use crate::error::CryptoError;

/// Argon2id parameters for vault key derivation.
///
/// These defaults balance security and usability:
/// - 64 MiB memory: resistant to GPU attacks, runs in ~0.5s on modern hardware
/// - 3 iterations: OWASP recommended minimum for Argon2id
/// - 4 parallelism: uses multiple cores
const ARGON2_MEMORY_KIB: u32 = 65_536; // 64 MiB
const ARGON2_ITERATIONS: u32 = 3;
const ARGON2_PARALLELISM: u32 = 4;
const ARGON2_OUTPUT_LEN: usize = 32; // 256-bit key for XChaCha20-Poly1305

/// Derive an encryption key from a passphrase using Argon2id.
///
/// Returns a 32-byte key wrapped in `SecretBox` for automatic
/// zeroize-on-drop. Callers use `key.expose_secret()` at the
/// precise point where the raw bytes are needed.
pub fn derive_key(passphrase: &[u8], salt: &[u8; 16]) -> Result<SecretBox<[u8; 32]>, CryptoError> {
    let params = Params::new(
        ARGON2_MEMORY_KIB,
        ARGON2_ITERATIONS,
        ARGON2_PARALLELISM,
        Some(ARGON2_OUTPUT_LEN),
    )
    .map_err(|e| CryptoError::KeyGeneration(format!("argon2 params: {e}")))?;

    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut key = [0u8; 32];
    argon2
        .hash_password_into(passphrase, salt, &mut key)
        .map_err(|e| CryptoError::KeyGeneration(format!("argon2 hash: {e}")))?;

    let secret = SecretBox::new(Box::new(key));
    // Zeroize the stack copy — SecretBox owns the heap copy now
    key.zeroize();
    Ok(secret)
}

/// Generate a random 16-byte salt for Argon2id.
pub fn generate_salt() -> [u8; 16] {
    use rand::RngCore;
    let mut salt = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    salt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_key_deterministic() {
        use secrecy::ExposeSecret;
        let salt = [1u8; 16];
        let key1 = derive_key(b"password", &salt).unwrap();
        let key2 = derive_key(b"password", &salt).unwrap();
        assert_eq!(key1.expose_secret(), key2.expose_secret());
    }

    #[test]
    fn test_derive_key_different_passphrase() {
        use secrecy::ExposeSecret;
        let salt = [1u8; 16];
        let key1 = derive_key(b"password1", &salt).unwrap();
        let key2 = derive_key(b"password2", &salt).unwrap();
        assert_ne!(key1.expose_secret(), key2.expose_secret());
    }

    #[test]
    fn test_derive_key_different_salt() {
        use secrecy::ExposeSecret;
        let key1 = derive_key(b"password", &[1u8; 16]).unwrap();
        let key2 = derive_key(b"password", &[2u8; 16]).unwrap();
        assert_ne!(key1.expose_secret(), key2.expose_secret());
    }

    #[test]
    fn test_generate_salt_is_random() {
        let s1 = generate_salt();
        let s2 = generate_salt();
        assert_ne!(s1, s2);
    }
}
