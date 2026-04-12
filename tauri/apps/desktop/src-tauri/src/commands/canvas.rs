use tauri::State;

use springtale_core::canvas::{CanvasState, CanvasUpdate};

use crate::state::AppState;

/// Get computed connection graph.
#[tauri::command]
pub async fn get_connections(
    state: State<'_, AppState>,
) -> Result<Vec<springtale_runtime::operations::canvas::Connection>, String> {
    springtale_runtime::operations::canvas::compute_connections(&state.runtime)
        .await
        .map_err(|e| e.to_string())
}

/// Get the current canvas state snapshot.
#[tauri::command]
pub async fn get_canvas_state(state: State<'_, AppState>) -> Result<CanvasState, String> {
    Ok(springtale_runtime::operations::canvas::get_canvas(&state.runtime).await)
}

/// Apply a canvas update.
#[tauri::command]
pub async fn update_canvas(
    state: State<'_, AppState>,
    update: CanvasUpdate,
) -> Result<CanvasState, String> {
    Ok(springtale_runtime::operations::canvas::update_canvas(&state.runtime, update).await)
}
