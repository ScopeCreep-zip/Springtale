use std::path::Path;
use std::sync::Arc;

use tauri::State;

use crate::runtime_guard::require_runtime;
use crate::state::AppState;

/// Prepare for travel — create encrypted backup then wipe local data.
///
/// Per ARCHITECTURE.md §2.6:
/// 1. Exports encrypted backup to user-specified location
/// 2. Wipes all local data (vault, database, config)
/// 3. Leaves minimal installation with no data
///
/// The travel passphrase crosses IPC once, is passed directly to
/// the crypto function for KDF, then dropped. Never stored.
#[tauri::command]
pub async fn travel_prepare(
    state: State<'_, AppState>,
    passphrase: String,
    backup_path: String,
) -> Result<(), String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    let store = Arc::clone(&rt.store);
    // Drop the guard before spawning blocking work to avoid holding
    // the read lock across the spawn_blocking boundary.
    drop(guard);

    tokio::task::spawn_blocking(move || {
        let vault_path = springtale_store::paths::default_vault_path();
        let db_path = springtale_store::paths::default_db_path();
        let config_path = springtale_store::paths::default_config_path();
        let backup = Path::new(&backup_path);

        springtale_runtime::operations::travel::prepare(
            &vault_path,
            &db_path,
            &config_path,
            backup,
            passphrase.as_bytes(),
            store.as_ref(),
        )
        .map_err(|e| format!("travel prepare failed: {e}"))
    })
    .await
    .map_err(|e| format!("travel prepare failed: {e}"))??;

    // Exit — local data is gone, backup is saved
    std::process::exit(0);
}

/// Restore from travel backup — decrypt and write vault, database, config.
///
/// Per ARCHITECTURE.md §2.6: "On arrival: restore from backup via
/// QR code or encrypted file."
///
/// After restore, the user must unlock the vault normally. The app
/// should be restarted to re-initialize AppState with restored data.
#[tauri::command]
pub async fn travel_restore(passphrase: String, backup_path: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let vault_path = springtale_store::paths::default_vault_path();
        let db_path = springtale_store::paths::default_db_path();
        let config_path = springtale_store::paths::default_config_path();
        let backup = Path::new(&backup_path);

        springtale_runtime::operations::travel::restore(
            backup,
            &vault_path,
            &db_path,
            &config_path,
            passphrase.as_bytes(),
        )
        .map_err(|e| format!("travel restore failed: {e}"))
    })
    .await
    .map_err(|e| format!("travel restore failed: {e}"))?
}
