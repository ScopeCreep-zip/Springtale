//! Connector installation — manifest validation, signature verification, WASM install.

use crate::error::OperationError;
use crate::state::RuntimeState;

/// Install a connector manifest — validates and registers in the store.
///
/// The manifest is validated for structure (name, version, no wildcard hosts).
/// If a signature is present, it's logged but verification is deferred to
/// Phase 2 (requires author public key registry).
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

    // Verify Ed25519 signature if present using trusted author registry
    verify_manifest_sig_if_present(&manifest, &*state.store).await?;

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

    // Verify signature if present
    verify_manifest_sig_if_present(&manifest, &*state.store).await?;

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
    state
        .store
        .store_wasm_binary(
            &registered_name,
            &wasm_bytes,
            &manifest_json,
            wasm_hash,
            &manifest.author,
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

/// Verify a manifest's Ed25519 signature if present.
///
/// Looks up the author's trusted public key from config store
/// (`trusted-author:{author_name}` → `{ "pubkey": "hex..." }`).
/// If the author is not in the trusted registry, logs a warning
/// but does not reject — unsigned/unknown-author connectors are
/// allowed but flagged. This matches the TOFU (Trust On First Use)
/// model: the first install establishes trust.
pub(super) async fn verify_manifest_sig_if_present(
    manifest: &springtale_connector::ConnectorManifest,
    store: &dyn springtale_store::StorageBackend,
) -> Result<(), OperationError> {
    let Some(ref _sig) = manifest.signature else {
        return Ok(()); // unsigned — no verification needed
    };

    let author_key_entry = format!("trusted-author:{}", manifest.author);
    match store.get_config(&author_key_entry).await {
        Ok(Some(key_json)) => {
            let key_data: serde_json::Value = serde_json::from_str(&key_json)
                .map_err(|e| OperationError::Validation(format!("invalid author key JSON: {e}")))?;

            let pubkey_hex = key_data
                .get("pubkey")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    OperationError::Validation("author key entry missing 'pubkey' field".into())
                })?;

            let pubkey_bytes = hex::decode(pubkey_hex).map_err(|e| {
                OperationError::Validation(format!("invalid author pubkey hex: {e}"))
            })?;

            let pubkey_arr: [u8; 32] = pubkey_bytes
                .try_into()
                .map_err(|_| OperationError::Validation("author pubkey must be 32 bytes".into()))?;

            let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&pubkey_arr)
                .map_err(|e| OperationError::Validation(format!("invalid author pubkey: {e}")))?;

            springtale_connector::manifest::verify::verify_manifest_signature(
                manifest,
                &verifying_key,
            )
            .map_err(|e| {
                OperationError::Validation(format!("signature verification failed: {e}"))
            })?;

            tracing::info!(
                connector = %manifest.name,
                author = %manifest.author,
                "manifest signature verified"
            );
        }
        _ => {
            tracing::warn!(
                connector = %manifest.name,
                author = %manifest.author,
                "manifest is signed but author not in trusted registry"
            );
        }
    }

    Ok(())
}
