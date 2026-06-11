//! Crypto-agile signature algorithm tag.
//!
//! Per NIST IR 8547 (PQC transition) and CNSA 2.0, Ed25519 is deprecated
//! 2030 and disallowed 2035. To honour CISA Secure-by-Design we ship the
//! algorithm identifier inside every signed artifact today, so the
//! verifier can dispatch by algorithm and we can roll a hybrid Ed25519 +
//! ML-DSA-65 path before the 2030 deadline without breaking older
//! manifests.
//!
//! `Ed25519` is the only variant currently produced. A future
//! `Ed25519MlDsa65` variant — gated behind a `pq-signature` feature
//! flag — will appear in a SemVer-minor bump well before 2030. See
//! `docs/security/CRYPTO-INVENTORY.md` for the migration plan.

use serde::{Deserialize, Serialize};

/// Identifier for the signature scheme used to sign an artifact.
///
/// Stored as the `signature_alg` field on every signable type
/// (connector manifest today, vault format and capability tokens
/// soon). Unknown variants fail closed at deserialise time.
#[derive(
    Debug,
    Default,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
    specta::Type,
)]
#[serde(rename_all = "snake_case")]
pub enum SignatureAlgorithm {
    /// Ed25519 over canonical JSON. Default for manifests today.
    #[default]
    Ed25519,
}

impl SignatureAlgorithm {
    /// Stable wire string. Use this when emitting CBOM / SBOM stanzas
    /// or audit-log entries — `Debug` format is for humans, this is
    /// the machine-readable form.
    pub fn as_wire(&self) -> &'static str {
        match self {
            Self::Ed25519 => "ed25519",
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn default_is_ed25519() {
        assert_eq!(SignatureAlgorithm::default(), SignatureAlgorithm::Ed25519);
    }

    #[test]
    fn wire_format_is_stable() {
        assert_eq!(SignatureAlgorithm::Ed25519.as_wire(), "ed25519");
    }

    #[test]
    fn serializes_to_snake_case() {
        let json = serde_json::to_string(&SignatureAlgorithm::Ed25519).unwrap();
        assert_eq!(json, "\"ed25519\"");
    }

    #[test]
    fn deserializes_from_snake_case() {
        let alg: SignatureAlgorithm = serde_json::from_str("\"ed25519\"").unwrap();
        assert_eq!(alg, SignatureAlgorithm::Ed25519);
    }

    #[test]
    fn rejects_unknown_algorithm() {
        let err = serde_json::from_str::<SignatureAlgorithm>("\"sha1-rsa\"");
        assert!(err.is_err(), "unknown algorithms must fail closed");
    }
}
