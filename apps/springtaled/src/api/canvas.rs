use axum::Json;
use axum::extract::State;

use springtale_core::canvas::CanvasState;
use springtale_runtime::operations;

use super::state::AppState;

/// Get computed connection graph.
pub async fn get_connections(State(state): State<AppState>) -> impl axum::response::IntoResponse {
    match operations::canvas::compute_connections(&state.runtime).await {
        Ok(conns) => Json(serde_json::json!({ "connections": conns })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

/// Get the current canvas state snapshot.
pub async fn get_canvas(State(state): State<AppState>) -> Json<CanvasState> {
    let canvas = operations::canvas::get_canvas(&state.runtime).await;
    Json(canvas)
}
