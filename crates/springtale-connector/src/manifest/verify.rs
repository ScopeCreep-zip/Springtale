use ed25519_dalek::{Signature, VerifyingKey};
use springtale_crypto::signature::SignatureAlgorithm;

use super::types::ConnectorManifest;
use crate::error::ConnectorError;

/// Verify a manifest's signature, dispatching on `manifest.signature_alg`.
///
/// The signature covers the canonical JSON of all manifest fields EXCEPT
/// the signature field itself. This ensures that the manifest has not been
/// tampered with since the author signed it.
///
/// Today only `SignatureAlgorithm::Ed25519` is implemented. The dispatch
/// indirection is the crypto-agility hook for the 2030 deadline (NIST
/// IR 8547) — see `docs/security/CRYPTO-INVENTORY.md`.
pub fn verify_manifest_signature(
    manifest: &ConnectorManifest,
    author_public_key: &VerifyingKey,
) -> Result<(), ConnectorError> {
    match manifest.signature_alg {
        SignatureAlgorithm::Ed25519 => verify_ed25519(manifest, author_public_key),
    }
}

fn verify_ed25519(
    manifest: &ConnectorManifest,
    author_public_key: &VerifyingKey,
) -> Result<(), ConnectorError> {
    let sig_hex = manifest
        .signature
        .as_ref()
        .ok_or_else(|| ConnectorError::ManifestInvalid("missing signature".into()))?;

    let sig_bytes = hex::decode(sig_hex)
        .map_err(|e| ConnectorError::ManifestInvalid(format!("invalid signature hex: {e}")))?;

    let sig_arr: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| ConnectorError::ManifestInvalid("signature must be 64 bytes".into()))?;

    let signature = Signature::from_bytes(&sig_arr);

    // Build the signable payload: all fields except signature
    let signable = signable_manifest_json(manifest)?;

    springtale_crypto::signature::verify::verify_canonical_json(
        author_public_key,
        &signable,
        &signature,
    )
    .map_err(|_| ConnectorError::SignatureInvalid)
}

/// Validate a manifest's structure (no signature check).
///
/// Checks: name is not empty, version is not empty, capabilities are valid.
pub fn verify_manifest(manifest: &ConnectorManifest) -> Result<(), ConnectorError> {
    if manifest.name.is_empty() {
        return Err(ConnectorError::ManifestInvalid("name is empty".into()));
    }
    if manifest.version.is_empty() {
        return Err(ConnectorError::ManifestInvalid("version is empty".into()));
    }
    if manifest.author.is_empty() {
        return Err(ConnectorError::ManifestInvalid("author is empty".into()));
    }

    // Check for wildcard hosts in NetworkOutbound
    for cap in &manifest.capabilities {
        if let super::types::Capability::NetworkOutbound { host } = cap {
            if host.contains('*') {
                return Err(ConnectorError::ManifestInvalid(format!(
                    "NetworkOutbound wildcards are not allowed: {host}"
                )));
            }
            if host.is_empty() {
                return Err(ConnectorError::ManifestInvalid(
                    "NetworkOutbound host is empty".into(),
                ));
            }
        }
    }

    Ok(())
}

/// Build the JSON value used for signing — all manifest fields except `signature`.
///
/// Shared with [`super::sign::sign_manifest`] so both sides cover the same bytes.
pub fn signable_manifest_json(
    manifest: &ConnectorManifest,
) -> Result<serde_json::Value, ConnectorError> {
    let mut json =
        serde_json::to_value(manifest).map_err(|e| ConnectorError::Serialization(e.to_string()))?;

    // Remove the signature field before signing/verifying
    if let serde_json::Value::Object(ref mut map) = json {
        map.remove("signature");
    }

    Ok(json)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::manifest::types::{Capability, ConnectorManifest};
    use springtale_crypto::identity::keypair::Keypair;
    use springtale_crypto::signature::sign::sign_canonical_json;

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
            wasm_hash: None,
            signature_alg: SignatureAlgorithm::default(),
            signature: None,
        }
    }

    fn sign_manifest(manifest: &mut ConnectorManifest, keypair: &Keypair) {
        let signable = signable_manifest_json(manifest).unwrap();
        let sig = sign_canonical_json(keypair, &signable).unwrap();
        manifest.signature = Some(hex::encode(sig.to_bytes()));
    }

    #[test]
    fn test_verify_manifest_structure() {
        let manifest = test_manifest();
        assert!(verify_manifest(&manifest).is_ok());
    }

    #[test]
    fn test_verify_rejects_empty_name() {
        let mut manifest = test_manifest();
        manifest.name = String::new();
        assert!(verify_manifest(&manifest).is_err());
    }

    #[test]
    fn test_verify_rejects_wildcard_host() {
        let mut manifest = test_manifest();
        manifest.capabilities = vec![Capability::NetworkOutbound {
            host: "*.example.com".into(),
        }];
        assert!(verify_manifest(&manifest).is_err());
    }

    #[test]
    fn test_verify_signature_valid() {
        let keypair = Keypair::generate().unwrap();
        let mut manifest = test_manifest();
        sign_manifest(&mut manifest, &keypair);

        let result = verify_manifest_signature(&manifest, keypair.verifying_key());
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_signature_tampered() {
        let keypair = Keypair::generate().unwrap();
        let mut manifest = test_manifest();
        sign_manifest(&mut manifest, &keypair);

        // Tamper with the manifest
        manifest.description = "tampered".into();

        let result = verify_manifest_signature(&manifest, keypair.verifying_key());
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_signature_wrong_key() {
        let keypair1 = Keypair::generate().unwrap();
        let keypair2 = Keypair::generate().unwrap();
        let mut manifest = test_manifest();
        sign_manifest(&mut manifest, &keypair1);

        let result = verify_manifest_signature(&manifest, keypair2.verifying_key());
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_signature_missing() {
        let keypair = Keypair::generate().unwrap();
        let manifest = test_manifest(); // no signature set
        let result = verify_manifest_signature(&manifest, keypair.verifying_key());
        assert!(result.is_err());
    }
}
