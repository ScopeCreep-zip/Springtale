use tauri::State;

use crate::runtime_guard::require_runtime;
use crate::state::AppState;

/// Get bot status: running state, connector/rule/formation counts.
#[tauri::command]
pub async fn bot_status(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();

    let registry = rt.registry.read().await;
    let connector_count = registry.list().len();
    drop(registry);

    let engine = rt.engine.read().await;
    let rule_count = engine.list_rules().len();
    drop(engine);

    let formation_count = rt
        .store
        .list_formations()
        .await
        .map(|f| f.len())
        .unwrap_or(0);

    Ok(serde_json::json!({
        "running": true,
        "connectors": connector_count,
        "rules": rule_count,
        "formations": formation_count,
    }))
}

/// Get bot memory stats (session count).
#[tauri::command]
pub async fn bot_memory(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    let sessions = rt.store.list_sessions().await.map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "session_count": sessions.len(),
    }))
}
