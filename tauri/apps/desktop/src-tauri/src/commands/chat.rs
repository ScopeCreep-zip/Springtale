//! In-app chat command — injects a message into the embedded bot loop.
//!
//! The desktop runs the same `Bot` the daemon does (built in
//! `state::init_runtime`). This command is the in-app chat ingress: it
//! pushes an `IncomingMessage` tagged with the synthetic `in-app` connector;
//! the bot's reply comes back to the frontend as a `chat-message` event
//! (emitted by the response dispatcher in `state::build_in_process_bot`).
//!
//! Fire-and-forget: returns once queued. Mirrors the dashboard's
//! `POST /chat` + SSE shape so the shared `ChatPanel` works on both surfaces.

use tauri::State;

use crate::state::AppState;

/// The synthetic connector + session id for in-app desktop chat.
const IN_APP: &str = "in-app";

#[tauri::command]
#[specta::specta]
pub async fn send_chat_message(
    state: State<'_, AppState>,
    text: String,
    session: Option<String>,
) -> Result<(), String> {
    let text = text.trim().to_owned();
    if text.is_empty() {
        return Err("empty message".to_owned());
    }
    let session = session.unwrap_or_else(|| IN_APP.to_owned());

    let tx = {
        let guard = state.bot_msg_tx.read().await;
        guard.clone()
    };
    let Some(tx) = tx else {
        tracing::warn!("[in-app-chat] message dropped — bot loop not started (vault locked?)");
        return Err("Vault is locked — unlock to chat.".to_owned());
    };

    tracing::info!(session = %session, text = %text, "[in-app-chat] → bot");
    tx.send(springtale_bot::IncomingMessage {
        user_id: IN_APP.to_owned(),
        channel_id: session,
        text,
        source_connector: IN_APP.to_owned(),
        raw: serde_json::json!({ "origin": "in-app" }),
    })
    .await
    .map_err(|_| "bot runtime unavailable".to_owned())?;

    Ok(())
}
