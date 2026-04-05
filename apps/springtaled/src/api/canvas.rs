use axum::Json;
use axum::extract::State;

use springtale_core::canvas::{CanvasState, CanvasUpdate};
use springtale_runtime::operations;

use super::state::AppState;

/// Get the current canvas state snapshot.
pub async fn get_canvas(State(state): State<AppState>) -> Json<CanvasState> {
    let canvas = operations::canvas::get_canvas(&state.runtime).await;
    Json(canvas)
}

/// Apply a canvas update — used by the bot orchestrator or internal API.
pub async fn update_canvas(
    State(state): State<AppState>,
    Json(update): Json<CanvasUpdate>,
) -> Json<CanvasState> {
    let snapshot = operations::canvas::update_canvas(&state.runtime, update).await;
    Json(snapshot)
}
