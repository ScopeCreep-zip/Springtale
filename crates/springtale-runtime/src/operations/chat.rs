//! In-app chat ingest.
//!
//! One entry point for "a person typed a message into the app": trim,
//! reject empty, wrap in the synthetic `in-app` connector envelope, and
//! hand it to the bot runtime's chat channel. Every surface that owns a
//! chat panel calls this rather than rebuilding the envelope.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::OperationError;
use crate::state::RuntimeState;

/// The synthetic connector name for in-app chat. Reply routing in the
/// response dispatcher branches on this exact value.
pub const IN_APP_CONNECTOR: &str = "in-app";

/// The single local in-app session/user. The daemon binds loopback and is
/// single-tenant, so one stable session id is correct.
pub const IN_APP_SESSION: &str = "in-app";

/// A message arriving from an in-app chat panel.
#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct IncomingMessage {
    /// The user's message.
    pub text: String,
    /// Optional session id; defaults to the single local in-app session.
    #[serde(default)]
    pub session: Option<String>,
}

/// Acknowledgement that a chat message was queued for the bot.
#[derive(Debug, Clone, Serialize, Type)]
pub struct ChatAccepted {
    /// Always `"queued"` — the reply arrives over the chat stream.
    pub status: String,
    /// Session the message was queued under.
    pub session: String,
}

/// Queue an in-app chat message for the bot runtime.
///
/// Fire-and-forget: the bot processes asynchronously and the reply
/// arrives over the chat broadcast the surface subscribes to.
pub async fn ingest(
    state: &RuntimeState,
    msg: IncomingMessage,
) -> Result<ChatAccepted, OperationError> {
    let text = msg.text.trim().to_owned();
    if text.is_empty() {
        return Err(OperationError::Validation("empty message".to_owned()));
    }
    let session = msg.session.unwrap_or_else(|| IN_APP_SESSION.to_owned());

    let envelope = springtale_connector::chat::ChatMessage::chat(
        IN_APP_CONNECTOR,
        session.clone(),
        IN_APP_SESSION,
        text,
        serde_json::json!({ "origin": "in-app" }),
    );

    state.chat_tx.send(envelope).await.map_err(|e| {
        tracing::error!(error = %e, "in-app chat: bot channel closed");
        OperationError::Rule("bot runtime unavailable".to_owned())
    })?;

    Ok(ChatAccepted {
        status: "queued".to_owned(),
        session,
    })
}
