//! Canvas operations — get and update A2UI state.

use springtale_core::canvas::{CanvasState, CanvasUpdate};

use crate::state::RuntimeState;

/// Get the current canvas state.
pub async fn get_canvas(state: &RuntimeState) -> CanvasState {
    state.canvas.read().await.clone()
}

/// Apply a canvas update — mutates state and broadcasts to subscribers.
pub async fn update_canvas(state: &RuntimeState, update: CanvasUpdate) -> CanvasState {
    let mut canvas = state.canvas.write().await;
    canvas.apply(&update);
    let snapshot = canvas.clone();

    // Broadcast to SSE/Tauri event subscribers
    let _ = state.canvas_tx.send(update);

    snapshot
}
