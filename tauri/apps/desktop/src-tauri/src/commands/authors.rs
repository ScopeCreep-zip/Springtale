use tauri::State;

use crate::runtime_guard::require_runtime;
use crate::state::AppState;

/// List all trusted authors.
#[tauri::command]
pub async fn list_authors(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    let configs = rt.store.list_config().await.map_err(|e| e.to_string())?;

    let authors: Vec<serde_json::Value> = configs
        .into_iter()
        .filter_map(|(key, value)| {
            let name = key.strip_prefix("trusted-author:")?.to_owned();
            let data: serde_json::Value = serde_json::from_str(&value).ok()?;
            Some(serde_json::json!({
                "name": name,
                "pubkey": data.get("pubkey").and_then(|v| v.as_str()).unwrap_or(""),
            }))
        })
        .collect();

    Ok(serde_json::json!({ "authors": authors }))
}

/// Add a trusted author.
#[tauri::command]
pub async fn add_author(
    state: State<'_, AppState>,
    name: String,
    pubkey: String,
) -> Result<(), String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();

    let bytes = hex::decode(&pubkey).map_err(|e| format!("invalid pubkey hex: {e}"))?;
    if bytes.len() != 32 {
        return Err("pubkey must be 32 bytes (64 hex chars)".into());
    }

    let key = format!("trusted-author:{name}");
    let value = serde_json::json!({ "pubkey": pubkey }).to_string();
    rt.store
        .set_config(&key, &value)
        .await
        .map_err(|e| e.to_string())
}

/// Remove a trusted author.
#[tauri::command]
pub async fn remove_author(state: State<'_, AppState>, name: String) -> Result<(), String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    let key = format!("trusted-author:{name}");
    rt.store
        .delete_config(&key)
        .await
        .map_err(|e| e.to_string())
}
