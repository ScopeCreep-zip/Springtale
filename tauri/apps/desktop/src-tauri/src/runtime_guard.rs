//! Deferred runtime guard — the Stronghold pattern.
//!
//! The desktop app's runtime starts as `None` and is populated after
//! the user unlocks the vault (which provides the passphrase needed to
//! derive the DB encryption key). Every IPC command that touches the
//! runtime calls [`require_runtime`] to get a read guard, receiving a
//! clean `"Vault is locked"` error if the user hasn't unlocked yet.
//!
//! This matches tauri-plugin-stronghold's `get_stronghold()` which
//! returns `Error::StrongholdNotInitialized` when the vault entry is
//! absent. Same pattern, different naming.

use std::sync::Arc;

use tokio::sync::RwLock;

/// The runtime state, wrapped in an `Option` so it can start as `None`
/// (vault locked) and be populated after unlock.
pub type DeferredRuntime = Arc<RwLock<Option<springtale_runtime::RuntimeState>>>;

/// Acquire a read guard on the runtime, returning a user-facing error
/// if the vault is still locked.
///
/// Usage in every IPC command that needs the runtime:
/// ```ignore
/// let guard = require_runtime(&state.runtime).await?;
/// let rt = guard.as_ref().unwrap(); // safe: require_runtime checked
/// ```
pub async fn require_runtime(
    deferred: &DeferredRuntime,
) -> Result<tokio::sync::RwLockReadGuard<'_, Option<springtale_runtime::RuntimeState>>, String> {
    let guard = deferred.read().await;
    if guard.is_none() {
        return Err("Vault is locked — unlock to continue".into());
    }
    Ok(guard)
}
