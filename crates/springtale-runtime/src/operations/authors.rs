//! Trusted connector-author operations.
//!
//! The `trusted-author:` config-key prefix and the Ed25519 public-key
//! length check live here, once, so every surface (HTTP, CLI, IPC)
//! validates a key the same way.

use serde::{Deserialize, Serialize};
use specta::Type;
use springtale_store::StorageBackend;

use crate::error::OperationError;

/// Config-key prefix under which trusted authors are stored.
pub const TRUSTED_AUTHOR_PREFIX: &str = "trusted-author:";

/// Length in bytes of an Ed25519 public key.
pub const ED25519_PUBKEY_LEN: usize = 32;

/// A trusted connector-manifest author.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct TrustedAuthor {
    /// Author name — the config key suffix.
    pub name: String,
    /// Hex-encoded Ed25519 public key.
    pub pubkey: String,
}

/// Request body for adding a trusted author.
#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct AddAuthorRequest {
    /// Hex-encoded Ed25519 public key (64 hex chars).
    pub pubkey: String,
}

/// List every trusted author.
pub async fn list(store: &dyn StorageBackend) -> Result<Vec<TrustedAuthor>, OperationError> {
    let configs = store.list_config().await.map_err(OperationError::Store)?;

    Ok(configs
        .into_iter()
        .filter_map(|(key, value)| {
            let name = key.strip_prefix(TRUSTED_AUTHOR_PREFIX)?;
            let data: serde_json::Value = serde_json::from_str(&value).ok()?;
            Some(TrustedAuthor {
                name: name.to_owned(),
                pubkey: data.get("pubkey")?.as_str()?.to_owned(),
            })
        })
        .collect())
}

/// Add a trusted author after validating the public key.
///
/// The key must be valid hex and exactly 32 bytes — anything else is a
/// validation error, never a stored half-key.
pub async fn add(
    store: &dyn StorageBackend,
    name: &str,
    pubkey: &str,
) -> Result<TrustedAuthor, OperationError> {
    if name.is_empty() {
        return Err(OperationError::Validation(
            "author name is empty".to_owned(),
        ));
    }
    let decoded = hex::decode(pubkey)
        .map_err(|_| OperationError::Validation("pubkey is not valid hex".to_owned()))?;
    if decoded.len() != ED25519_PUBKEY_LEN {
        return Err(OperationError::Validation(format!(
            "pubkey must be {ED25519_PUBKEY_LEN} bytes, got {}",
            decoded.len()
        )));
    }

    let key = format!("{TRUSTED_AUTHOR_PREFIX}{name}");
    let value = serde_json::json!({ "pubkey": pubkey }).to_string();
    store
        .set_config(&key, &value)
        .await
        .map_err(OperationError::Store)?;

    tracing::info!(author = %name, "trusted author added");
    Ok(TrustedAuthor {
        name: name.to_owned(),
        pubkey: pubkey.to_owned(),
    })
}

/// Remove a trusted author.
pub async fn remove(store: &dyn StorageBackend, name: &str) -> Result<(), OperationError> {
    let key = format!("{TRUSTED_AUTHOR_PREFIX}{name}");
    store
        .delete_config(&key)
        .await
        .map_err(OperationError::Store)?;
    tracing::info!(author = %name, "trusted author removed");
    Ok(())
}
