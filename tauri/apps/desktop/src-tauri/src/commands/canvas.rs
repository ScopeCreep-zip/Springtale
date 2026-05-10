use tauri::ipc::Channel;
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

/// F4 + E10: Tauri IPC streaming subscription to live canvas updates.
///
/// Mirrors the web dashboard's `/canvas/stream` SSE path
/// (`apps/springtaled/src/api/canvas_stream.rs`) using `tauri::ipc::Channel<T>`
/// — the right Tauri 2 primitive for high-rate streaming (vs broadcast
/// `app.emit()`). Subscribes to the in-process `runtime.canvas_tx` broadcast
/// and forwards every update to the channel until the channel is dropped
/// or the broadcast lags / closes.
///
/// Per `docs/guide/colony-canvas.md §10` data flow: both surfaces consume
/// `LiveFormationReader` through the `DataProvider` abstraction. This
/// command is the desktop's equivalent of the web's SSE connection.
#[tauri::command]
pub async fn subscribe_canvas(
    state: State<'_, AppState>,
    channel: Channel<CanvasUpdate>,
) -> Result<(), String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    let mut rx = rt.canvas_tx.subscribe();
    // Spawn a forwarder so `subscribe_canvas` returns immediately. The
    // forwarder lives until the channel is dropped (frontend disconnect)
    // or the broadcast errors out.
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(update) => {
                    if channel.send(update).is_err() {
                        // Frontend dropped the channel — stop forwarding.
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "canvas channel forwarder lagged");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
    Ok(())
}
