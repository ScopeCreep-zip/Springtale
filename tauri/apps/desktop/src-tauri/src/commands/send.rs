use tauri::State;

use crate::runtime_guard::require_runtime;
use crate::state::AppState;
use springtale_runtime::operations::cross_channel::{self, SendOutcome, SendRequest};

/// Send a message through a specific connector to a channel.
#[tauri::command]
pub async fn send_message(
    state: State<'_, AppState>,
    req: SendRequest,
) -> Result<SendOutcome, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    cross_channel::send(rt, req)
        .await
        .map_err(|e| e.to_string())
}
