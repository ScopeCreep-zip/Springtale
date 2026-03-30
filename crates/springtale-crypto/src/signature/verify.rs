use ed25519_dalek::{Signature, Verifier, VerifyingKey};

use super::sign::canonical_json;
use crate::error::CryptoError;

/// Verify an Ed25519 signature over raw bytes.
pub fn verify_bytes(
    public_key: &VerifyingKey,
    data: &[u8],
    signature: &Signature,
) -> Result<(), CryptoError> {
    public_key
        .verify(data, signature)
        .map_err(|_| CryptoError::InvalidSignature)
}

/// Verify an Ed25519 signature over canonical JSON.
///
/// The JSON value is canonicalized (keys sorted) before verification,
/// ensuring the same object always verifies regardless of key ordering.
pub fn verify_canonical_json(
    public_key: &VerifyingKey,
    value: &serde_json::Value,
    signature: &Signature,
) -> Result<(), CryptoError> {
    let canonical = canonical_json(value)?;
    verify_bytes(public_key, canonical.as_bytes(), signature)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::keypair::Keypair;
    use crate::signature::sign::sign_canonical_json;
    use serde_json::json;

    #[test]
    fn test_verify_valid_signature() {
        let keypair = Keypair::generate();
        assert!(keypair.is_ok());
        let kp = keypair.as_ref().ok();

        let data = b"hello springtale";
        let sig = kp.map(|k| k.sign(data));

        let result = kp.and_then(|k| {
            sig.as_ref()
                .map(|s| verify_bytes(k.verifying_key(), data, s))
        });
        assert!(result.is_some_and(|r| r.is_ok()));
    }

    #[test]
    fn test_verify_tampered_data_fails() {
        let keypair = Keypair::generate();
        assert!(keypair.is_ok());
        let kp = keypair.as_ref().ok();

        let data = b"hello springtale";
        let sig = kp.map(|k| k.sign(data));

        let tampered = b"hello tampered";
        let result = kp.and_then(|k| {
            sig.as_ref()
                .map(|s| verify_bytes(k.verifying_key(), tampered, s))
        });
        assert!(result.is_some_and(|r| r.is_err()));
    }

    #[test]
    fn test_verify_wrong_key_fails() {
        let keypair1 = Keypair::generate();
        let keypair2 = Keypair::generate();
        assert!(keypair1.is_ok());
        assert!(keypair2.is_ok());

        let data = b"hello springtale";
        let sig = keypair1.as_ref().ok().map(|k| k.sign(data));

        // Verify with wrong key
        let result = keypair2.as_ref().ok().and_then(|k| {
            sig.as_ref()
                .map(|s| verify_bytes(k.verifying_key(), data, s))
        });
        assert!(result.is_some_and(|r| r.is_err()));
    }

    #[test]
    fn test_verify_canonical_json_different_key_order() {
        let keypair = Keypair::generate();
        assert!(keypair.is_ok());
        let kp = keypair.as_ref().ok();

        // Sign with one key ordering
        let value1 = json!({"b": 2, "a": 1});
        let sig = kp.and_then(|k| sign_canonical_json(k, &value1).ok());

        // Verify with different key ordering — should still pass
        let value2 = json!({"a": 1, "b": 2});
        let result = kp.and_then(|k| {
            sig.as_ref()
                .map(|s| verify_canonical_json(k.verifying_key(), &value2, s))
        });
        assert!(result.is_some_and(|r| r.is_ok()));
    }
}
