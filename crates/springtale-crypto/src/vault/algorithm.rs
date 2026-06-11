//! Crypto-agile vault format metadata.
//!
//! The decrypted vault payload carries the AEAD and KDF algorithm tags
//! so a future migration (e.g. XChaCha20-Poly1305 → AES-256-GCM, or
//! Argon2id → Argon2 with stronger params) can roll without ambiguity
//! about which scheme produced the bytes. Per NIST IR 8547 + CISA
//! Secure-by-Design: ship the algorithm identifier with every
//! ciphertext.
//!
//! The on-disk wire layout is unchanged — `[salt][nonce][ciphertext]`
//! still looks like random bytes; the algorithm tags live inside the
//! authenticated ciphertext so an attacker can neither read them nor
//! tamper with them without detection.
//!
//! `Aead::XChaCha20Poly1305` + `Kdf::Argon2id` are the only variants
//! currently produced. New variants land as SemVer-minor additions;
//! unknown values fail closed at deserialise time.

use serde::{Deserialize, Serialize};

/// AEAD scheme used to seal the vault plaintext.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AeadAlgorithm {
    /// XChaCha20-Poly1305 (RFC 8439 + draft-irtf-cfrg-xchacha-03).
    #[default]
    XChaCha20Poly1305,
}

/// KDF used to derive the AEAD key from the user's passphrase.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KdfAlgorithm {
    /// Argon2id (RFC 9106). OWASP Argon2 cheat-sheet recommended KDF.
    #[default]
    Argon2id,
}

/// Argon2id parameters in their plaintext, machine-readable form.
///
/// Bound to the ciphertext via the AEAD tag, so an attacker cannot
/// downgrade the cost factor without invalidating the ciphertext.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Argon2Params {
    /// Memory cost, in KiB.
    pub memory_kib: u32,
    /// Time cost (iterations).
    pub iterations: u32,
    /// Parallelism (lanes).
    pub parallelism: u32,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn aead_default() {
        assert_eq!(AeadAlgorithm::default(), AeadAlgorithm::XChaCha20Poly1305);
    }

    #[test]
    fn kdf_default() {
        assert_eq!(KdfAlgorithm::default(), KdfAlgorithm::Argon2id);
    }

    #[test]
    fn aead_serializes_snake_case() {
        let json = serde_json::to_string(&AeadAlgorithm::XChaCha20Poly1305).unwrap();
        assert_eq!(json, "\"x_cha_cha20_poly1305\"");
    }

    #[test]
    fn unknown_aead_rejected() {
        let r = serde_json::from_str::<AeadAlgorithm>("\"aes_gcm\"");
        assert!(r.is_err());
    }

    #[test]
    fn argon2_params_roundtrip() {
        let p = Argon2Params {
            memory_kib: 65536,
            iterations: 3,
            parallelism: 4,
        };
        let j = serde_json::to_string(&p).unwrap();
        let p2: Argon2Params = serde_json::from_str(&j).unwrap();
        assert_eq!(p, p2);
    }
}
