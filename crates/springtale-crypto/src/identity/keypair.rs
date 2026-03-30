use ed25519_dalek::{SigningKey, VerifyingKey};
use secrecy::{ExposeSecret, SecretBox};
use zeroize::Zeroize;

use super::node_id::NodeId;
use crate::error::CryptoError;

/// A secret wrapper around Ed25519 signing key bytes.
///
/// `SigningKey` implements `ZeroizeOnDrop` but not `Zeroize`, so we wrap
/// the raw 32-byte secret key material in `SecretBox<[u8; 32]>` and
/// reconstruct the `SigningKey` only at call sites that need it.
///
/// This type intentionally does NOT implement `Debug` or `Clone`.
pub struct Keypair {
    secret_bytes: SecretBox<[u8; 32]>,
    verifying_key: VerifyingKey,
}

impl Keypair {
    /// Generate a new random keypair using the OS CSPRNG.
    pub fn generate() -> Result<Self, CryptoError> {
        let mut csprng = rand::rngs::OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();

        let mut secret_bytes = signing_key.to_bytes();
        let keypair = Self {
            secret_bytes: SecretBox::new(Box::new(secret_bytes)),
            verifying_key,
        };

        // Zeroize the local copy — SecretBox now owns the bytes
        secret_bytes.zeroize();

        Ok(keypair)
    }

    /// Reconstruct from raw secret bytes (e.g., loaded from vault).
    pub fn from_secret_bytes(mut bytes: [u8; 32]) -> Result<Self, CryptoError> {
        let signing_key = SigningKey::from_bytes(&bytes);
        let verifying_key = signing_key.verifying_key();

        let keypair = Self {
            secret_bytes: SecretBox::new(Box::new(bytes)),
            verifying_key,
        };

        // Zeroize the input copy
        bytes.zeroize();

        Ok(keypair)
    }

    /// Get the public verifying key.
    pub fn verifying_key(&self) -> &VerifyingKey {
        &self.verifying_key
    }

    /// Get the node ID (public key as bytes).
    pub fn node_id(&self) -> NodeId {
        NodeId::from(self.verifying_key)
    }

    /// Sign a message. The signing key is exposed only for this call.
    pub fn sign(&self, message: &[u8]) -> ed25519_dalek::Signature {
        // SECURITY: expose needed for Ed25519 signing operation
        let secret = self.secret_bytes.expose_secret();
        let signing_key = SigningKey::from_bytes(secret);
        use ed25519_dalek::Signer;
        signing_key.sign(message)
    }

    /// Export the secret bytes for vault persistence.
    /// The caller MUST zeroize the returned bytes after use.
    pub fn expose_secret_bytes(&self) -> &[u8; 32] {
        // SECURITY: expose needed for vault persistence
        self.secret_bytes.expose_secret()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::Verifier;

    #[test]
    fn test_generate_and_sign() {
        let keypair = Keypair::generate();
        assert!(keypair.is_ok());
        let keypair = keypair.ok();
        assert!(keypair.is_some());
        let keypair = keypair.as_ref();

        let message = b"hello springtale";
        let signature = keypair.map(|kp| kp.sign(message));
        assert!(signature.is_some());

        let kp = keypair;
        let sig = signature;
        assert!(
            kp.and_then(|k| sig.map(|s| k.verifying_key().verify(message, &s)))
                .is_some_and(|r| r.is_ok())
        );
    }

    #[test]
    fn test_roundtrip_from_bytes() {
        let original = Keypair::generate();
        assert!(original.is_ok());
        let original = original.ok();

        let bytes = original.as_ref().map(|kp| *kp.expose_secret_bytes());
        let restored = bytes.and_then(|b| Keypair::from_secret_bytes(b).ok());

        assert_eq!(
            original.as_ref().map(|kp| kp.node_id()),
            restored.as_ref().map(|kp| kp.node_id()),
        );
    }

    #[test]
    fn test_node_id_is_32_bytes() {
        let keypair = Keypair::generate();
        assert!(keypair.is_ok());
        let kp = keypair.ok();
        assert_eq!(kp.as_ref().map(|k| k.node_id().as_bytes().len()), Some(32));
    }
}
