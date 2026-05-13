use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;
use tauri_specta::Event;

use crate::state::AppState;

/// Emitted after a successful `create_vault` or `unlock_vault` so the
/// frontend can close the passphrase overlay and start loading data.
/// Unit payload — the frontend re-queries vault status via the
/// `get_vault_status` command.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct VaultUnlocked;

/// Emitted after `lock_vault` and from the autolock timer when the
/// inactivity threshold expires. Unit payload; the frontend zeroizes
/// any in-memory secrets and re-shows the passphrase overlay.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct VaultLocked;

/// Create a new vault with a passphrase.
///
/// After creating the vault file and identity keypair, derives the DB
/// encryption key from the passphrase and initializes the full runtime.
/// The frontend's vault overlay closes on success.
#[tauri::command]
#[specta::specta]
pub async fn create_vault(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    passphrase: String,
) -> Result<springtale_runtime::operations::vault::VaultStatus, String> {
    let vault_path = springtale_store::paths::default_vault_path();

    let (vault, status) = springtale_runtime::operations::vault::create_vault(
        &vault_path,
        passphrase.as_bytes(),
    )
    .map_err(|e| e.to_string())?;

    // Derive the DB encryption key from the same passphrase, then
    // initialize the runtime (opens DB, loads rules, connectors, etc.).
    let db_key = if crate::state::detect_encryption_needed() {
        Some(springtale_crypto::token::derive_db_encryption_key_hex(
            passphrase.as_bytes(),
        ))
    } else {
        None
    };
    // Zeroize passphrase — we have the derived key and the Vault object
    drop(passphrase);

    // W1.F — wire a `ChannelApprovalGate` so the sentinel prompts the
    // user via the ApprovalCard overlay instead of silently denying
    // destructive actions.
    let gate = crate::state::build_approval_gate(app.clone(), state.approval_dispatcher.clone());
    crate::state::init_runtime(&state.runtime, &state.scheduler, db_key, Some(gate)).await?;

    let mut vault_guard = state.vault.lock().await;
    *vault_guard = Some(vault);

    let _ = VaultUnlocked.emit(&app);
    Ok(status)
}

/// Unlock the vault with a passphrase.
///
/// Opens the vault file, derives the DB encryption key, and initializes
/// the full runtime. If the runtime was previously torn down by
/// `lock_vault`, this re-creates it with a fresh DB connection.
#[tauri::command]
#[specta::specta]
pub async fn unlock_vault(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    passphrase: String,
) -> Result<springtale_runtime::operations::vault::VaultStatus, String> {
    let vault_path = springtale_store::paths::default_vault_path();

    let (vault, status) = springtale_runtime::operations::vault::open_vault(
        &vault_path,
        passphrase.as_bytes(),
    )
    .map_err(|e| e.to_string())?;

    // Derive the DB key from the same passphrase, then init the runtime.
    let db_key = if crate::state::detect_encryption_needed() {
        Some(springtale_crypto::token::derive_db_encryption_key_hex(
            passphrase.as_bytes(),
        ))
    } else {
        None
    };
    drop(passphrase);

    // Only init if runtime isn't already running (avoid double-init on
    // repeated unlock attempts before the frontend closes the overlay).
    {
        let guard = state.runtime.read().await;
        if guard.is_some() {
            // Already initialized — just store the vault and return.
            drop(guard);
            let mut vault_guard = state.vault.lock().await;
            *vault_guard = Some(vault);
            let _ = VaultUnlocked.emit(&app);
            return Ok(status);
        }
    }

    // W1.F — wire a `ChannelApprovalGate` so the sentinel prompts the
    // user via the ApprovalCard overlay instead of silently denying
    // destructive actions.
    let gate = crate::state::build_approval_gate(app.clone(), state.approval_dispatcher.clone());
    crate::state::init_runtime(&state.runtime, &state.scheduler, db_key, Some(gate)).await?;

    let mut vault_guard = state.vault.lock().await;
    *vault_guard = Some(vault);

    let _ = VaultUnlocked.emit(&app);
    Ok(status)
}

/// Lock the vault — zeroes key material in memory and tears down the
/// runtime so no DB access is possible until re-unlock.
#[tauri::command]
#[specta::specta]
pub async fn lock_vault(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    // Zero vault key material
    {
        let mut vault_guard = state.vault.lock().await;
        if let Some(ref mut vault) = *vault_guard {
            springtale_runtime::operations::vault::lock_vault(vault);
        }
        *vault_guard = None;
    }

    // Tear down runtime — closes DB handle, drops connectors.
    // Next unlock re-creates everything from scratch.
    {
        let mut rt = state.runtime.write().await;
        *rt = None;
    }

    let _ = VaultLocked.emit(&app);
    Ok(())
}

/// Get the current vault status.
#[tauri::command]
#[specta::specta]
pub async fn get_vault_status(
    state: State<'_, AppState>,
) -> Result<springtale_runtime::operations::vault::VaultStatus, String> {
    let vault_guard = state.vault.lock().await;
    Ok(springtale_runtime::operations::vault::get_vault_status(
        &*vault_guard,
    ))
}
