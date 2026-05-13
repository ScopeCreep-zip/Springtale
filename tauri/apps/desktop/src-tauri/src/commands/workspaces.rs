//! External-workspace directory IPC (D1).
//!
//! Thin: validates args, defers to
//! `springtale-runtime::operations::workspaces` for everything.
//! The runtime returns sizes-only privacy-shaped rows.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, State};
use tauri_specta::Event;

use springtale_cooperation::cadence::AgentId;
use springtale_runtime::operations::workspaces::{
    delete_workspace as runtime_delete, list_workspaces as runtime_list,
    preview_onboard_url as runtime_preview_onboard_url,
    scan_workspaces as runtime_scan, start_onboard_stream as runtime_start_onboard_stream,
    upsert_workspace_manual, OnDiscoveryCallback, WorkspaceInfo,
};

use crate::runtime_guard::require_runtime;
use crate::state::AppState;

/// Track D specta event. Emitted once per chat discovered by the
/// short-lived onboarding stream. The frontend filters by
/// `session_id` (the id it minted when starting the stream) so two
/// concurrent picker mounts don't cross-pollinate.
#[derive(Clone, Debug, Serialize, Deserialize, Type, Event)]
pub struct ChatDiscovered {
    pub session_id: String,
    pub workspace_key: String,
    pub display_name: String,
    pub kind: String,
    pub metadata_json: Option<String>,
    /// `true` if the chat passed the `/start <payload>` filter — i.e.
    /// it is the user's own onboarding tap. The picker auto-selects on
    /// `matched=true`; other discoveries (currently always matched, but
    /// kept for forward compatibility) only populate the dropdown.
    pub matched: bool,
}

/// List every workspace in a formation, optionally filtered by
/// connector. Newest-first.
#[tauri::command]
#[specta::specta]
pub async fn list_workspaces(
    state: State<'_, AppState>,
    formation_id: String,
    connector_filter: Option<String>,
) -> Result<Vec<WorkspaceInfo>, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    runtime_list(&rt.store, &formation_id, connector_filter.as_deref())
        .await
        .map_err(|e| format!("list_workspaces: {e}"))
}

/// 🔍 Scan — call the connector's `discover_destinations` action
/// + return the formation's updated directory.
#[tauri::command]
#[specta::specta]
pub async fn scan_workspaces(
    state: State<'_, AppState>,
    formation_id: String,
    connector_name: String,
) -> Result<Vec<WorkspaceInfo>, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    runtime_scan(rt, &formation_id, &connector_name)
        .await
        .map_err(|e| format!("scan_workspaces: {e}"))
}

#[tauri::command]
#[specta::specta]
pub async fn delete_workspace(
    state: State<'_, AppState>,
    formation_id: String,
    workspace_key: String,
) -> Result<(), String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    runtime_delete(&rt.store, &formation_id, &workspace_key)
        .await
        .map_err(|e| format!("delete_workspace: {e}"))
}

/// 🎯 Onboard — pre-deploy onboarding URL resolver.
///
/// Connector-agnostic. Hands the deploy form's connector config off
/// to the connector factory, dispatches the connector's `onboard_url`
/// action, returns the resolved URL. Connectors without an
/// `onboard_url` action surface as an error.
#[tauri::command]
#[specta::specta]
pub async fn preview_onboard_url(
    _state: State<'_, AppState>,
    connector_name: String,
    config: serde_json::Value,
    payload: Option<String>,
) -> Result<String, String> {
    let payload = payload.unwrap_or_else(|| "springtale-onboard".to_owned());
    runtime_preview_onboard_url(&connector_name, config, &payload)
        .await
        .map_err(|e| format!("preview_onboard_url: {e}"))
}

/// ✏️ Manual entry — register a workspace the user typed in.
#[tauri::command]
#[specta::specta]
pub async fn upsert_workspace_manual_cmd(
    state: State<'_, AppState>,
    formation_id: String,
    workspace_key: String,
    display_name: String,
    connector_name: String,
    kind: String,
) -> Result<(), String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    // For manual entry, the "entered_by" agent isn't tied to a
    // logged-in identity yet (Phase 3 brings auth). Use a fresh
    // random AgentId so the provenance record is well-formed —
    // distinguishes manual entries from each other in the audit
    // trail even without a stable user-identity layer.
    let entered_by = AgentId::default();
    upsert_workspace_manual(
        &rt.store,
        &formation_id,
        entered_by,
        workspace_key,
        display_name,
        connector_name,
        kind,
    )
    .await
    .map_err(|e| format!("upsert_workspace_manual: {e}"))
}

/// 🎯 Onboard — start the 60s pre-deploy auto-onboarding stream.
///
/// Spawns a tokio task in the runtime that polls the connector's
/// `discover_destinations` action every 2 seconds, looking for
/// messages whose payload matches the frontend-issued `payload`
/// (typically `"springtale-onboard"` — the value embedded in the
/// `t.me/<bot>?start=...` deep link). Each match fires a
/// [`ChatDiscovered`] event tagged with the same `session_id` the
/// frontend supplied so multiple concurrent picker instances stay
/// distinct.
///
/// The cancel sender is parked on `AppState.onboard_sessions` so
/// `cancel_onboard_stream` (or implicit cleanup on next start with
/// the same `session_id`) can shut the task down within one poll
/// interval.
#[tauri::command]
#[specta::specta]
pub async fn start_onboard_stream(
    state: State<'_, AppState>,
    app: AppHandle,
    session_id: String,
    connector_name: String,
    config: serde_json::Value,
    payload: Option<String>,
) -> Result<(), String> {
    let payload = payload.unwrap_or_else(|| "springtale-onboard".to_owned());

    // Replace any prior session under this id — keeps the AppState
    // map bounded if the user double-clicks Onboard mid-stream.
    if let Some(prev) = state.onboard_sessions.lock().await.remove(&session_id) {
        let _ = prev.send(true);
    }

    let app_for_cb = app.clone();
    let sid = session_id.clone();
    let on_discover: OnDiscoveryCallback =
        Arc::new(move |info: WorkspaceInfo, matched: bool| {
            let event = ChatDiscovered {
                session_id: sid.clone(),
                workspace_key: info.workspace_key,
                display_name: info.display_name,
                kind: info.kind,
                metadata_json: info.metadata_json,
                matched,
            };
            if let Err(e) = event.emit(&app_for_cb) {
                tracing::warn!(error = %e, "ChatDiscovered emit failed");
            }
        });

    let cancel_tx =
        runtime_start_onboard_stream(connector_name, config, payload, on_discover)
            .map_err(|e| format!("start_onboard_stream: {e}"))?;

    state
        .onboard_sessions
        .lock()
        .await
        .insert(session_id, cancel_tx);
    Ok(())
}

/// Tear down an active onboarding stream early.
///
/// Called by the frontend's `onCleanup` when the user closes the
/// deploy form mid-stream. Idempotent — sending to a vacant session
/// id is a no-op.
#[tauri::command]
#[specta::specta]
pub async fn cancel_onboard_stream(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    if let Some(tx) = state.onboard_sessions.lock().await.remove(&session_id) {
        let _ = tx.send(true);
    }
    Ok(())
}
