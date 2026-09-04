//! Connector installation — manifest validation, signature verification, WASM install.

use crate::error::OperationError;
use crate::state::RuntimeState;

/// Install a connector manifest — validates and registers in the store.
///
/// The manifest is validated for structure (name, version, no wildcard hosts)
/// and must carry a signature by an author in the trusted-author registry —
/// [`verify_manifest_signature`] rejects unsigned and unknown-author
/// manifests outright. First-party native connectors register through
/// `inventory` and never enter this path.
/// [`springtale_sentinel::Sentinel::check_toxic_pairs`] additionally gates
/// dangerous capability combinations.
pub async fn install_connector(
    state: &RuntimeState,
    manifest: springtale_connector::ConnectorManifest,
) -> Result<String, OperationError> {
    // Validate manifest structure
    springtale_connector::manifest::verify::verify_manifest(&manifest)
        .map_err(|e| OperationError::Validation(format!("manifest invalid: {e}")))?;

    // Check for toxic capability pairs (e.g., KeychainRead + NetworkOutbound)
    springtale_sentinel::Sentinel::check_toxic_pairs(&manifest.capabilities)
        .map_err(|e| OperationError::Validation(format!("toxic capability pair: {e}")))?;

    // Signing is required for every manifest that reaches this path.
    verify_manifest_signature(&manifest, &*state.store).await?;

    let manifest_json = serde_json::to_string(&manifest)
        .map_err(|e| OperationError::Serialization(e.to_string()))?;

    let row = springtale_store::schema::connectors::ConnectorRow {
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        author: manifest.author.clone(),
        description: manifest.description.clone(),
        manifest_json,
        enabled: true,
        installed_at: chrono::Utc::now(),
    };

    state.store.register_connector(&row).await?;

    // Fold any community roles this manifest declares into the shared
    // `RoleRegistry` so formation reload can reconstruct members that
    // reference them (§14.4 / Phase 21).
    crate::cooperation::register_manifest_roles(&state.role_registry, &manifest);

    let name = manifest.name;
    tracing::info!(connector = %name, "connector manifest registered");
    Ok(name)
}

/// Install a WASM connector from binary + manifest.
///
/// Verifies manifest structure, signature, and WASM hash, then installs
/// into the in-memory registry (sandboxed) and persists the binary to
/// the store so it survives restarts.
pub async fn install_wasm_connector(
    state: &RuntimeState,
    wasm_bytes: Vec<u8>,
    manifest: springtale_connector::ConnectorManifest,
) -> Result<String, OperationError> {
    // Validate manifest structure
    springtale_connector::manifest::verify::verify_manifest(&manifest)
        .map_err(|e| OperationError::Validation(format!("manifest invalid: {e}")))?;

    // Check for toxic capability pairs (e.g., FilesystemRead + NetworkOutbound)
    springtale_sentinel::Sentinel::check_toxic_pairs(&manifest.capabilities)
        .map_err(|e| OperationError::Validation(format!("toxic capability pair: {e}")))?;

    // Verify the (required) signature. Return the pubkey hex we used so
    // we can pin it to the persisted row (Phase-7 audit Finding #1).
    let trusted_pubkey_hex = verify_manifest_signature(&manifest, &*state.store).await?;

    // Verify WASM hash
    let wasm_hash = manifest.wasm_hash.as_deref().ok_or_else(|| {
        OperationError::Validation("WASM connector manifest must include wasm_hash".into())
    })?;
    springtale_connector::wasm::WasmEngine::verify_wasm_hash(&wasm_bytes, wasm_hash)
        .map_err(|e| OperationError::Validation(format!("WASM hash verification failed: {e}")))?;

    // Install in registry using the shared WASM engine (same epoch ticker)
    // and the shared per-tier `InstancePre` cache (§16).
    let registered_name = {
        let mut registry = state.registry.write().await;
        registry
            .install_wasm(
                state.wasm_engine.clone(),
                &wasm_bytes,
                manifest.clone(),
                springtale_connector::wasm::SandboxLimits::default(),
                state.wasm_tier_cache.clone(),
            )
            .map_err(|e| OperationError::Connector(format!("WASM install failed: {e}")))?
    };

    // Fold any community roles declared in the manifest into the shared
    // registry (Phase 21). For WASM connectors this is the main path —
    // the role definitions live in the manifest, not in Rust code.
    crate::cooperation::register_manifest_roles(&state.role_registry, &manifest);

    // Persist to store for restart survival
    let manifest_json = serde_json::to_string(&manifest)
        .map_err(|e| OperationError::Serialization(e.to_string()))?;
    // Whole-codebase audit Finding #1: persist the install-time
    // pubkey + signature in trust-anchor columns so the boot
    // re-verifier can fail closed on a swap-the-whole-bundle attack
    // against the SQLite store (TUF §4). Both are always present:
    // `verify_manifest_signature` has already rejected unsigned installs.
    let pinned_pubkey = trusted_pubkey_hex;
    let pinned_sig = manifest.signature.clone().unwrap_or_default();
    state
        .store
        .store_wasm_binary(
            &registered_name,
            &wasm_bytes,
            &manifest_json,
            wasm_hash,
            &manifest.author,
            &pinned_pubkey,
            &pinned_sig,
        )
        .await?;

    // Clear any prior removal flag
    let removed_key = format!("connector-removed:{registered_name}");
    let _ = state.store.delete_config(&removed_key).await;

    tracing::info!(
        connector = %registered_name,
        author = %manifest.author,
        wasm_hash = wasm_hash,
        "WASM connector installed (sandboxed + persisted)"
    );

    Ok(registered_name)
}

/// Verify a manifest's Ed25519 signature against the trusted-author
/// registry and return the pubkey hex that verified it.
///
/// Signing is required. There is no unsigned path, no override flag and
/// no fallback: an unsigned manifest or one signed by an author absent
/// from the registry is a `Validation` error. The author's public key
/// is read from config (`trusted-author:{author}` → `{ "pubkey": "hex" }`);
/// developers register their own identity with `springtale author add
/// --self` and sign with `springtale connector sign <manifest.toml>`.
///
/// The returned hex is pinned into the persisted row so the boot
/// re-verifier can re-check it (Phase-7 audit Finding #1).
pub(super) async fn verify_manifest_signature(
    manifest: &springtale_connector::ConnectorManifest,
    store: &dyn springtale_store::StorageBackend,
) -> Result<String, OperationError> {
    let Some(_) = manifest.signature.as_ref() else {
        return Err(OperationError::Validation(format!(
            "manifest for {} is unsigned; sign it with `springtale connector sign <manifest.toml>`",
            manifest.name
        )));
    };

    let author_key_entry = format!("trusted-author:{}", manifest.author);
    let Ok(Some(key_json)) = store.get_config(&author_key_entry).await else {
        return Err(OperationError::Validation(format!(
            "manifest for {} is signed by unknown author {}; run `springtale author add {} <pubkey-hex>` if you trust them",
            manifest.name, manifest.author, manifest.author
        )));
    };

    let key_data: serde_json::Value = serde_json::from_str(&key_json)
        .map_err(|e| OperationError::Validation(format!("invalid author key JSON: {e}")))?;

    let pubkey_hex = key_data
        .get("pubkey")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            OperationError::Validation("author key entry missing 'pubkey' field".into())
        })?;

    let pubkey_bytes = hex::decode(pubkey_hex)
        .map_err(|e| OperationError::Validation(format!("invalid author pubkey hex: {e}")))?;

    let pubkey_arr: [u8; 32] = pubkey_bytes
        .try_into()
        .map_err(|_| OperationError::Validation("author pubkey must be 32 bytes".into()))?;

    let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&pubkey_arr)
        .map_err(|e| OperationError::Validation(format!("invalid author pubkey: {e}")))?;

    springtale_connector::manifest::verify::verify_manifest_signature(manifest, &verifying_key)
        .map_err(|e| OperationError::Validation(format!("signature verification failed: {e}")))?;

    tracing::info!(
        connector = %manifest.name,
        author = %manifest.author,
        "manifest signature verified"
    );
    Ok(pubkey_hex.to_owned())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use springtale_connector::manifest::{
        Capability, ConnectorManifest, SignatureAlgorithm, sign_manifest,
    };
    use springtale_crypto::identity::keypair::Keypair;
    use springtale_store::StorageBackend;
    use springtale_store::backend::sqlite::SqliteBackend;

    fn wasm_manifest() -> ConnectorManifest {
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

    #[tokio::test]
    async fn test_verify_manifest_signature_unsigned_returns_validation() {
        let store = SqliteBackend::open_in_memory().unwrap();
        let manifest = wasm_manifest();

        let err = verify_manifest_signature(&manifest, &store)
            .await
            .unwrap_err();

        assert!(
            matches!(err, OperationError::Validation(ref m) if m.contains("unsigned")),
            "{err}"
        );
    }

    #[tokio::test]
    async fn test_verify_manifest_signature_unknown_author_returns_validation_naming_author() {
        let store = SqliteBackend::open_in_memory().unwrap();
        let keypair = Keypair::generate().unwrap();
        let mut manifest = wasm_manifest();
        sign_manifest(&mut manifest, &keypair).unwrap();

        let err = verify_manifest_signature(&manifest, &store)
            .await
            .unwrap_err();

        assert!(
            matches!(err, OperationError::Validation(ref m) if m.contains("test-author")),
            "{err}"
        );
    }

    #[tokio::test]
    async fn test_verify_manifest_signature_trusted_author_returns_pubkey_hex() {
        let store = SqliteBackend::open_in_memory().unwrap();
        let keypair = Keypair::generate().unwrap();
        let pubkey_hex = hex::encode(keypair.verifying_key().to_bytes());
        store
            .set_config(
                "trusted-author:test-author",
                &serde_json::json!({ "pubkey": pubkey_hex }).to_string(),
            )
            .await
            .unwrap();
        let mut manifest = wasm_manifest();
        sign_manifest(&mut manifest, &keypair).unwrap();

        let verified = verify_manifest_signature(&manifest, &store).await.unwrap();

        assert_eq!(verified, pubkey_hex);
    }
}
