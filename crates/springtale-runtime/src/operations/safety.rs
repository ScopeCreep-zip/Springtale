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

/// G5d — toggle the app's disguise mode. Persists the new state to
/// `safety_config` so the disguise survives a process restart
/// (critical for IPV survivors: opening the app under coercion must
/// not reveal the real surface). Returns the new active state for
/// the caller to thread into UI / tray-icon updates.
pub async fn set_disguise_active(
    state: &RuntimeState,
    active: bool,
) -> Result<bool, OperationError> {
    let mut config = get_safety_config(state).await?;
    config.disguise_active = active;
    config.updated_at = chrono::Utc::now();
    save_safety_config(state, config).await?;
    Ok(active)
}

/// G5d — apply a fresh disguise profile (app name + icon id). Atomic:
/// both fields update or neither does. Doesn't flip
/// `disguise_active` — the caller invokes `set_disguise_active`
/// separately so the choice of *which* disguise is decoupled from
/// *whether* to display it.
pub async fn set_disguise_profile(
    state: &RuntimeState,
    app_name: String,
    icon_id: String,
) -> Result<(), OperationError> {
    let mut config = get_safety_config(state).await?;
    config.disguise_app_name = app_name;
    config.disguise_icon_id = icon_id;
    config.updated_at = chrono::Utc::now();
    save_safety_config(state, config).await
}

/// G5d — update the panic-tap threshold (number of rapid title-bar
/// taps that trigger panic-wipe). `count = 0` disables the gesture.
/// Bounded `[0, 10]` so the user can't accidentally make panic-wipe
/// unreachable in a real emergency.
pub async fn set_panic_tap_count(
    state: &RuntimeState,
    count: u32,
) -> Result<u32, OperationError> {
    if count > 10 {
        return Err(OperationError::Validation(format!(
            "panic_tap_count {count} exceeds safe upper bound of 10 — survivors need the gesture to fire reliably under duress"
        )));
    }
    let mut config = get_safety_config(state).await?;
    config.panic_tap_count = count;
    config.updated_at = chrono::Utc::now();
    save_safety_config(state, config).await?;
    Ok(count)
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
