use tauri::State;

use crate::state::AppState;

/// Create a new vault with a passphrase.
#[tauri::command]
pub async fn create_vault(
    state: State<'_, AppState>,
    passphrase: String,
) -> Result<springtale_runtime::operations::vault::VaultStatus, String> {
    let vault_path = springtale_store::paths::default_vault_path();

    let (vault, status) = springtale_runtime::operations::vault::create_vault(
        &vault_path,
        passphrase.as_bytes(),
    )
    .map_err(|e| e.to_string())?;

    let mut vault_guard = state.vault.lock().await;
    *vault_guard = Some(vault);
    drop(passphrase);

    Ok(status)
}

/// Unlock the vault with a passphrase.
#[tauri::command]
pub async fn unlock_vault(
    state: State<'_, AppState>,
    passphrase: String,
) -> Result<springtale_runtime::operations::vault::VaultStatus, String> {
    let vault_path = springtale_store::paths::default_vault_path();

    let (vault, status) = springtale_runtime::operations::vault::open_vault(
        &vault_path,
        passphrase.as_bytes(),
    )
    .map_err(|e| e.to_string())?;

    let mut vault_guard = state.vault.lock().await;
    *vault_guard = Some(vault);
    drop(passphrase);

    Ok(status)
}

/// Lock the vault — zeroes key material in memory.
#[tauri::command]
pub async fn lock_vault(state: State<'_, AppState>) -> Result<(), String> {
    let mut vault_guard = state.vault.lock().await;
    if let Some(ref mut vault) = *vault_guard {
        springtale_runtime::operations::vault::lock_vault(vault);
    }
    *vault_guard = None;
    Ok(())
}

/// Get the current vault status.
#[tauri::command]
pub async fn get_vault_status(
    state: State<'_, AppState>,
) -> Result<springtale_runtime::operations::vault::VaultStatus, String> {
    let vault_guard = state.vault.lock().await;
    Ok(springtale_runtime::operations::vault::get_vault_status(&*vault_guard))
}
