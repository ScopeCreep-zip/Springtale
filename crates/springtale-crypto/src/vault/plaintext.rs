//! Crypto-agile vault plaintext envelope.
//!
//! What gets serialized AS the plaintext fed to the AEAD on save is no
//! longer a bare `HashMap<String, Vec<u8>>` — it's a `VaultPlaintext`
//! that pins the AEAD scheme, KDF, and KDF parameters. Authentic and
//! integrity-protected via the AEAD tag (downgrade attack ⇒ tag
//! verification fails).
//!
//! The on-disk wire layout stays `[salt][nonce][ciphertext]`. Only
//! the contents of the encrypted region change, so the "no magic
//! bytes / indistinguishable from random" property is preserved.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::algorithm::{AeadAlgorithm, Argon2Params, KdfAlgorithm};

/// Authenticated, encrypted payload of a vault region. Serialised via
/// `serde_json` and handed to the AEAD's `encrypt`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VaultPlaintext {
    /// AEAD that produced the surrounding ciphertext. Must match the
    /// scheme the verifier uses to decrypt — mismatch means downgrade
    /// or wrong key, both of which the AEAD tag would already catch;
    /// the tag exists for explicit error messages and audit logs.
    pub aead: AeadAlgorithm,

    /// KDF that derived the AEAD key from the passphrase.
    pub kdf: KdfAlgorithm,

    /// KDF parameters in plaintext-machine-readable form. Bound to the
    /// ciphertext via the AEAD tag, so an attacker cannot downgrade the
    /// cost factor without invalidating the ciphertext.
    pub kdf_params: Argon2Params,

    /// User-facing entries. Map of key → raw bytes.
    pub entries: HashMap<String, Vec<u8>>,
}

impl VaultPlaintext {
    /// Build a new plaintext envelope with the workspace defaults
    /// (XChaCha20-Poly1305 + Argon2id 64 MiB / 3 iter / 4 lanes).
    pub fn with_defaults(entries: HashMap<String, Vec<u8>>) -> Self {
        Self {
            aead: AeadAlgorithm::default(),
            kdf: KdfAlgorithm::default(),
            kdf_params: super::kdf::workspace_argon2_params(),
            entries,
        }
    }
}

/// Decode the entry map from a region's *already-decrypted* plaintext,
/// accepting both vault formats:
///
/// 1. **Current** — the crypto-agile [`VaultPlaintext`] envelope
///    (`{aead, kdf, kdf_params, entries}`). The AEAD/KDF tags are validated,
///    so a downgrade or forward-version vault fails closed.
/// 2. **Legacy (pre-envelope)** — the plaintext was the bare entry map
///    `{"key": [bytes], …}`, written before the envelope existed. Vaults in
///    this shape would otherwise fail to open with `unknown field …`.
///
/// Accepting the legacy shape is safe: callers only reach this *after* the
/// AEAD tag has authenticated the plaintext, so it's a genuine old vault, not
/// a tampered one. Such vaults transparently re-save in the new envelope on
/// the next [`super::Vault::save`]. The new format is tried first, so a
/// current envelope is never misread as the legacy map (its `aead` is a
/// string, which fails `Vec<u8>` decoding anyway).
pub(crate) fn decode_region_entries(
    plaintext: &[u8],
) -> Result<HashMap<String, Vec<u8>>, crate::error::CryptoError> {
    match serde_json::from_slice::<VaultPlaintext>(plaintext) {
        Ok(envelope) => {
            if envelope.aead != AeadAlgorithm::XChaCha20Poly1305
                || envelope.kdf != KdfAlgorithm::Argon2id
            {
                return Err(crate::error::CryptoError::VaultDecryptionFailed);
            }
            Ok(envelope.entries)
        }
        Err(_) => serde_json::from_slice::<HashMap<String, Vec<u8>>>(plaintext)
            .map_err(|e| crate::error::CryptoError::Serialization(e.to_string())),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn defaults_roundtrip() {
        let mut entries = HashMap::new();
        entries.insert("k".to_string(), b"v".to_vec());
        let p = VaultPlaintext::with_defaults(entries);

        let bytes = serde_json::to_vec(&p).unwrap();
        let p2: VaultPlaintext = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(p2.aead, AeadAlgorithm::XChaCha20Poly1305);
        assert_eq!(p2.kdf, KdfAlgorithm::Argon2id);
        assert_eq!(p2.kdf_params.memory_kib, 65_536);
        assert_eq!(p2.kdf_params.iterations, 3);
        assert_eq!(p2.kdf_params.parallelism, 4);
        assert_eq!(p2.entries.get("k"), Some(&b"v".to_vec()));
    }

    #[test]
    fn rejects_unknown_fields() {
        let bad = r#"{
            "aead": "x_cha_cha20_poly1305",
            "kdf": "argon2id",
            "kdf_params": { "memory_kib": 1, "iterations": 1, "parallelism": 1 },
            "entries": {},
            "extra": "field"
        }"#;
        let r = serde_json::from_str::<VaultPlaintext>(bad);
        assert!(r.is_err(), "unknown fields must fail closed");
    }

    #[test]
    fn decode_accepts_current_envelope() {
        let mut entries = HashMap::new();
        entries.insert("identity".to_string(), b"keypair-bytes".to_vec());
        let plaintext = serde_json::to_vec(&VaultPlaintext::with_defaults(entries)).unwrap();
        let decoded = decode_region_entries(&plaintext).unwrap();
        assert_eq!(decoded.get("identity"), Some(&b"keypair-bytes".to_vec()));
    }

    #[test]
    fn decode_accepts_legacy_flat_map() {
        // Exactly what the pre-envelope `save()` wrote: `serde_json::to_vec`
        // of the bare entry map. This is the format of the user's April vault.
        let mut entries = HashMap::new();
        entries.insert("identity".to_string(), b"keypair-bytes".to_vec());
        entries.insert("openai.api_key".to_string(), b"sk-xxx".to_vec());
        let legacy_plaintext = serde_json::to_vec(&entries).unwrap();

        let decoded = decode_region_entries(&legacy_plaintext).unwrap();
        assert_eq!(decoded.get("identity"), Some(&b"keypair-bytes".to_vec()));
        assert_eq!(decoded.get("openai.api_key"), Some(&b"sk-xxx".to_vec()));
    }

    #[test]
    fn decode_rejects_garbage() {
        assert!(decode_region_entries(b"not json at all").is_err());
    }
}
