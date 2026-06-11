//! Phase H4 — `subscribe_cooperation` Tauri IPC command.
//!
//! Mirrors `commands/canvas.rs::subscribe_canvas` verbatim. Subscribes
//! to `runtime.cooperation_tx` (Phase H2 broadcast bus) and forwards every
//! envelope to a per-window `tauri::ipc::Channel<CooperationEventEnvelope>`
//! until the channel drops or the broadcast lags / closes.
//!
//! Per E10: `Channel<T>` is the right Tauri 2 primitive for high-rate
//! streaming (vs `app.emit()`). Cooperation events fire ~5–20× per tick at
//! 30Hz across 4 formations — well above the threshold where Channel<T>
//! beats the broadcast emit path.

use tauri::State;
use tauri::ipc::Channel;

use springtale_cooperation::CooperationEventEnvelope;

use crate::runtime_guard::require_runtime;
use crate::state::AppState;

/// IPC streaming subscription to cooperation lifecycle envelopes.
///
/// Mirrors the web dashboard's `/cooperation/events` SSE endpoint
/// (`apps/springtaled/src/api/cooperation_stream.rs`). Every internal-
/// state cooperation event (intervention fired, sacrifice yielded, vote
/// opened, role transformed, member marked down, supervisor escalation,
/// pacing phase change, cascade hit, recovery action, surface deposit,
/// interference event, CFP/replan/commit outcome) reaches the desktop UI
/// through this channel.
#[tauri::command]
#[specta::specta]
pub async fn subscribe_cooperation(
    state: State<'_, AppState>,
    channel: Channel<CooperationEventEnvelope>,
) -> Result<(), String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    let mut rx = rt.cooperation_tx.subscribe();
    // Spawn a forwarder so `subscribe_cooperation` returns immediately.
    // Lives until the channel is dropped (frontend disconnect) or the
    // broadcast errors out.
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(envelope) => {
                    if channel.send(envelope).is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "cooperation channel forwarder lagged");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
    Ok(())
}
