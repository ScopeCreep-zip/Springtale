//! Manifest signing — the author-side twin of [`super::verify`].
//!
//! Signs exactly the bytes the verifier checks: the canonical JSON of
//! every manifest field except `signature` (see
//! [`super::verify::signable_manifest_json`]). Keeping both halves in
//! this crate guarantees the signer and verifier never drift apart.

use springtale_crypto::identity::keypair::Keypair;
use springtale_crypto::signature::SignatureAlgorithm;
use springtale_crypto::signature::sign::sign_canonical_json;

use super::types::ConnectorManifest;
use super::verify::signable_manifest_json;
use crate::error::ConnectorError;

/// Sign `manifest` with `keypair`, store the signature on the manifest
/// and return its hex encoding.
///
/// Dispatches on `manifest.signature_alg`; today only Ed25519 exists.
pub fn sign_manifest(
    manifest: &mut ConnectorManifest,
    keypair: &Keypair,
) -> Result<String, ConnectorError> {
    match manifest.signature_alg {
        SignatureAlgorithm::Ed25519 => {
            let signable = signable_manifest_json(manifest)?;
            let signature = sign_canonical_json(keypair, &signable)
                .map_err(|e| ConnectorError::Serialization(e.to_string()))?;
            let hex = hex::encode(signature.to_bytes());
            manifest.signature = Some(hex.clone());
            Ok(hex)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::manifest::types::Capability;
    use crate::manifest::verify::verify_manifest_signature;

    fn test_manifest() -> ConnectorManifest {
        ConnectorManifest {
            name: "connector-test".into(),
            version: "1.0.0".into(),
            author: "test-author".into(),
            description: "A test connector".into(),
            capabilities: vec![Capability::NetworkOutbound {
                host: "api.example.com".into(),
            }],
            triggers: vec![],
            actions: vec![],
            data_disclosure: vec![],
            roles: vec![],
            wasm_hash: Some("deadbeef".into()),
            signature_alg: SignatureAlgorithm::default(),
            signature: None,
        }
    }

    #[test]
    fn test_sign_manifest_sets_signature_that_verifies() {
        let keypair = Keypair::generate().unwrap();
        let mut manifest = test_manifest();

        let hex = sign_manifest(&mut manifest, &keypair).unwrap();

        assert_eq!(manifest.signature.as_deref(), Some(hex.as_str()));
        assert!(verify_manifest_signature(&manifest, keypair.verifying_key()).is_ok());
    }

    #[test]
    fn test_sign_manifest_survives_toml_round_trip() {
        // `springtale connector sign` writes the signed manifest back as
        // TOML and `connector install` parses it again: the signature
        // must still cover exactly what the verifier sees.
        let keypair = Keypair::generate().unwrap();
        let mut manifest = test_manifest();
        sign_manifest(&mut manifest, &keypair).unwrap();

        let toml_text = toml::to_string_pretty(&manifest).unwrap();
        let reparsed: ConnectorManifest = toml::from_str(&toml_text).unwrap();

        assert!(verify_manifest_signature(&reparsed, keypair.verifying_key()).is_ok());
    }
}
