//! Safety operations — config persistence and panic wipe.

use springtale_store::SafetyConfigRow;
use springtale_store::StorageBackend;

use crate::error::OperationError;
use crate::state::RuntimeState;

/// Default safety configuration — safe for IPV scenarios.
pub fn default_safety_config() -> SafetyConfigRow {
    SafetyConfigRow::default()
}

/// Get the current safety configuration (returns defaults if none saved).
pub async fn get_safety_config(state: &RuntimeState) -> Result<SafetyConfigRow, OperationError> {
    let row = state.store.get_safety_config().await?;
    Ok(row.unwrap_or_default())
}

/// Save safety configuration to store.
pub async fn save_safety_config(
    state: &RuntimeState,
    config: SafetyConfigRow,
) -> Result<(), OperationError> {
    state.store.upsert_safety_config(&config).await?;
    Ok(())
}

/// Emergency data destruction — wipes vault, database, config.
///
/// Per ARCHITECTURE.md §2.6: must complete within 3 seconds.
pub async fn panic_wipe(store: &dyn StorageBackend) -> Result<(), OperationError> {
    let vault_path = springtale_store::paths::default_vault_path();
    let config_path = springtale_store::paths::default_config_path();

    // Wipe vault file
    if vault_path.exists() {
        springtale_crypto::vault::wipe::wipe_vault_file(&vault_path)
            .map_err(|e| OperationError::Rule(format!("vault wipe failed: {e}")))?;
    }

    // Wipe SQLite
    store.panic_wipe().map_err(OperationError::Store)?;

    // Wipe config
    if config_path.exists() {
        let _ = springtale_crypto::vault::wipe::wipe_vault_file(&config_path);
    }

    Ok(())
}
