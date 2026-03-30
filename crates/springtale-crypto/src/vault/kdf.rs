use argon2::{Algorithm, Argon2, Params, Version};
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
/// Returns a 32-byte key suitable for XChaCha20-Poly1305.
/// The key is returned as a raw array — caller MUST zeroize after use.
pub fn derive_key(passphrase: &[u8], salt: &[u8; 16]) -> Result<[u8; 32], CryptoError> {
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

    Ok(key)
}

/// Generate a random 16-byte salt for Argon2id.
pub fn generate_salt() -> [u8; 16] {
    use rand::RngCore;
    let mut salt = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    salt
}

/// Zeroize a key after use.
pub fn zeroize_key(key: &mut [u8; 32]) {
    key.zeroize();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_key_deterministic() {
        let salt = [1u8; 16];
        let key1 = derive_key(b"password", &salt);
        let key2 = derive_key(b"password", &salt);
        assert!(key1.is_ok());
        assert!(key2.is_ok());
        assert_eq!(key1.ok(), key2.ok());
    }

    #[test]
    fn test_derive_key_different_passphrase() {
        let salt = [1u8; 16];
        let key1 = derive_key(b"password1", &salt);
        let key2 = derive_key(b"password2", &salt);
        assert!(key1.is_ok());
        assert!(key2.is_ok());
        assert_ne!(key1.ok(), key2.ok());
    }

    #[test]
    fn test_derive_key_different_salt() {
        let key1 = derive_key(b"password", &[1u8; 16]);
        let key2 = derive_key(b"password", &[2u8; 16]);
        assert!(key1.is_ok());
        assert!(key2.is_ok());
        assert_ne!(key1.ok(), key2.ok());
    }

    #[test]
    fn test_generate_salt_is_random() {
        let s1 = generate_salt();
        let s2 = generate_salt();
        assert_ne!(s1, s2);
    }
}
