use chrono::{DateTime, Utc};
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};

use crate::error::CryptoError;
use crate::identity::keypair::Keypair;
use crate::signature::sign::sign_canonical_json;
use crate::signature::verify::verify_canonical_json;

/// A signed, expiring capability grant.
///
/// Issued per-connector, per-task. Expires after task completion
/// (default 1 hour). Cannot be transferred or self-elevated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityToken {
    /// Who issued this token.
    pub issuer: String,

    /// Who this token is granted to (agent or connector name).
    pub subject: String,

    /// What capability is granted (e.g., "NetworkOutbound:api.kick.com").
    pub capability: String,

    /// When this token was issued.
    pub issued_at: DateTime<Utc>,

    /// When this token expires.
    pub expires_at: DateTime<Utc>,

    /// Ed25519 signature over the canonical JSON of the token fields.
    #[serde(with = "signature_serde")]
    pub signature: ed25519_dalek::Signature,
}

impl CapabilityToken {
    /// Issue a new capability token, signed by the issuer's keypair.
    pub fn issue(
        issuer_keypair: &Keypair,
        issuer_name: &str,
        subject: &str,
        capability: &str,
        duration: chrono::Duration,
    ) -> Result<Self, CryptoError> {
        let now = Utc::now();
        let expires = now + duration;

        // Build the signable payload (everything except the signature)
        let payload = serde_json::json!({
            "issuer": issuer_name,
            "subject": subject,
            "capability": capability,
            "issued_at": now.to_rfc3339(),
            "expires_at": expires.to_rfc3339(),
        });

        let signature = sign_canonical_json(issuer_keypair, &payload)?;

        Ok(Self {
            issuer: issuer_name.to_owned(),
            subject: subject.to_owned(),
            capability: capability.to_owned(),
            issued_at: now,
            expires_at: expires,
            signature,
        })
    }

    /// Verify the token's signature and check expiry.
    pub fn verify(&self, issuer_public_key: &VerifyingKey) -> Result<(), CryptoError> {
        // Check expiry first (cheap)
        if Utc::now() > self.expires_at {
            return Err(CryptoError::InvalidSignature); // expired
        }

        // Rebuild the signable payload
        let payload = serde_json::json!({
            "issuer": self.issuer,
            "subject": self.subject,
            "capability": self.capability,
            "issued_at": self.issued_at.to_rfc3339(),
            "expires_at": self.expires_at.to_rfc3339(),
        });

        verify_canonical_json(issuer_public_key, &payload, &self.signature)
    }

    /// Check if the token has expired.
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }
}

/// Serde support for ed25519_dalek::Signature (as hex string).
mod signature_serde {
    use ed25519_dalek::Signature;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(sig: &Signature, s: S) -> Result<S::Ok, S::Error> {
        hex::encode(sig.to_bytes()).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Signature, D::Error> {
        let hex_str = String::deserialize(d)?;
        let bytes = hex::decode(&hex_str).map_err(serde::de::Error::custom)?;
        let arr: [u8; 64] = bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom("signature must be 64 bytes"))?;
        Ok(Signature::from_bytes(&arr))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_issue_and_verify() {
        let keypair = Keypair::generate();
        assert!(keypair.is_ok());
        let kp = keypair.as_ref().ok();

        let token = kp.and_then(|k| {
            CapabilityToken::issue(
                k,
                "springtale",
                "connector-kick",
                "NetworkOutbound:api.kick.com",
                chrono::Duration::hours(1),
            )
            .ok()
        });
        assert!(token.is_some());

        let result = kp.and_then(|k| token.as_ref().map(|t| t.verify(k.verifying_key())));
        assert!(result.is_some_and(|r| r.is_ok()));
    }

    #[test]
    fn test_expired_token_fails() {
        let keypair = Keypair::generate();
        assert!(keypair.is_ok());
        let kp = keypair.as_ref().ok();

        // Issue with negative duration (already expired)
        let token = kp.and_then(|k| {
            CapabilityToken::issue(
                k,
                "springtale",
                "connector-kick",
                "NetworkOutbound:api.kick.com",
                chrono::Duration::hours(-1),
            )
            .ok()
        });
        assert!(token.is_some());
        assert!(token.as_ref().is_some_and(|t| t.is_expired()));

        let result = kp.and_then(|k| token.as_ref().map(|t| t.verify(k.verifying_key())));
        assert!(result.is_some_and(|r| r.is_err()));
    }

    #[test]
    fn test_wrong_key_fails() {
        let keypair1 = Keypair::generate();
        let keypair2 = Keypair::generate();
        assert!(keypair1.is_ok());
        assert!(keypair2.is_ok());

        let token = keypair1.as_ref().ok().and_then(|k| {
            CapabilityToken::issue(k, "springtale", "test", "test", chrono::Duration::hours(1)).ok()
        });

        let result = keypair2
            .as_ref()
            .ok()
            .and_then(|k| token.as_ref().map(|t| t.verify(k.verifying_key())));
        assert!(result.is_some_and(|r| r.is_err()));
    }
}
