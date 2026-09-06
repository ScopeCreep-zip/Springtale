//! In-app chat HTTP surface (W5).
//!
//! The desktop app, web dashboard, and mobile PWA all talk to the same bot
//! through one channel: `POST /chat` injects a message as if it arrived from
//! a connector named `in-app`, and `GET /chat/stream` streams the bot's
//! replies back over SSE. This reuses the existing chat spine end to end —
//! `ChatMessage` → `ai_fallback`/router → `OutgoingResponse` — with the
//! response dispatcher (`runtime/boot/bot.rs`) routing `in-app`-connector
//! replies to the broadcast this module subscribes to.
//!
//! Approvals for in-app tasks surface through the existing
//! `GET /approvals` + `POST /approvals/{id}` management API — the chat
//! panel renders the pending queue inline rather than receiving a card.

use std::convert::Infallible;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::sse::{Event, KeepAlive, Sse};
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;

use super::state::AppState;

/// The synthetic connector name for in-app chat. Reply routing in the
/// response dispatcher branches on this exact value.
pub const IN_APP_CONNECTOR: &str = "in-app";
/// The single local in-app session/user. The daemon binds loopback and is
/// single-tenant, so one stable session id is correct.
pub const IN_APP_SESSION: &str = "in-app";

/// A bot reply destined for the in-app chat panel.
#[derive(Debug, Clone, Serialize)]
pub struct ChatStreamMessage {
    /// Session id (channel) the reply belongs to.
    pub session: String,
    /// The reply text.
    pub text: String,
}

/// Request body for `POST /chat`.
#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    /// The user's message.
    pub text: String,
    /// Optional session id; defaults to the single local in-app session.
    #[serde(default)]
    pub session: Option<String>,
}

/// `POST /chat` — inject a chat message from the in-app panel.
///
/// Fire-and-forget: the bot processes asynchronously and the reply arrives
/// over `GET /chat/stream`. Returns `202 Accepted` once queued.
pub async fn send(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> impl IntoResponse {
    let text = req.text.trim().to_owned();
    if text.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "empty message" })),
        );
    }
    let session = req.session.unwrap_or_else(|| IN_APP_SESSION.to_owned());

    let msg = springtale_connector::chat::ChatMessage::chat(
        IN_APP_CONNECTOR,
        session.clone(),
        IN_APP_SESSION,
        text,
        serde_json::json!({ "origin": "in-app" }),
    );

    if let Err(e) = state.bot_msg_tx.send(msg).await {
        tracing::error!(error = %e, "in-app chat: bot channel closed");
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "bot runtime unavailable" })),
        );
    }

    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({ "status": "queued", "session": session })),
    )
}

/// `GET /chat/stream` — SSE stream of bot replies for the in-app panel.
///
/// Auth required (Bearer). Read-only. Each event's data is a
/// `ChatStreamMessage` JSON object.
pub async fn stream(
    State(state): State<AppState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.chat_tx.subscribe();
    let stream =
        tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(|result| match result {
            Ok(msg) => {
                let data = serde_json::to_string(&msg).unwrap_or_default();
                Some(Ok(Event::default().data(data)))
            }
            Err(_) => None, // lagged subscriber — skip
        });
    // Ends on lock so the connection releases its `AppState` clone.
    Sse::new(futures_util::StreamExt::take_until(stream, state.locked()))
        .keep_alive(KeepAlive::default())
}
