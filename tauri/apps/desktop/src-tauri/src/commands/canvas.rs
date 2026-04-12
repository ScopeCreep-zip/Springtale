use tauri::State;

use springtale_core::canvas::{CanvasState, CanvasUpdate};

use crate::runtime_guard::require_runtime;
use crate::state::AppState;

/// Get computed connection graph.
#[tauri::command]
pub async fn get_connections(
    state: State<'_, AppState>,
) -> Result<Vec<springtale_runtime::operations::canvas::Connection>, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::canvas::compute_connections(rt)
        .await
        .map_err(|e| e.to_string())
}

/// Get the current canvas state snapshot.
#[tauri::command]
pub async fn get_canvas_state(state: State<'_, AppState>) -> Result<CanvasState, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    Ok(springtale_runtime::operations::canvas::get_canvas(rt).await)
}

/// Apply a canvas update.
#[tauri::command]
pub async fn update_canvas(
    state: State<'_, AppState>,
    update: CanvasUpdate,
) -> Result<CanvasState, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    Ok(springtale_runtime::operations::canvas::update_canvas(rt, update).await)
}
