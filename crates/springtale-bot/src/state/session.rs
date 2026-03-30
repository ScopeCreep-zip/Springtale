use std::sync::Arc;

use springtale_store::StorageBackend;

use crate::error::BotError;

/// Unique key for a session: (user_id, channel_id).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionKey {
    pub user_id: String,
    pub channel_id: String,
}

/// Convenience wrapper around `SessionRow` for the bot runtime.
#[derive(Debug, Clone)]
pub struct Session {
    pub key: SessionKey,
    pub last_bot_message: Option<String>,
    pub pending_command: Option<String>,
    pub state_data: serde_json::Value,
}

impl Session {
    /// Create a new empty session for a user/channel pair.
    pub fn new(user_id: &str, channel_id: &str) -> Self {
        Self {
            key: SessionKey {
                user_id: user_id.into(),
                channel_id: channel_id.into(),
            },
            last_bot_message: None,
            pending_command: None,
            state_data: serde_json::json!({}),
        }
    }
}

/// Load a session from the store, or create a new one if not found.
pub async fn load_or_create_session(
    store: &Arc<dyn StorageBackend>,
    key: &SessionKey,
) -> Result<Session, BotError> {
    match store.get_session(&key.user_id, &key.channel_id).await? {
        Some(row) => {
            let state_data =
                serde_json::from_str(&row.state_data).unwrap_or_else(|_| serde_json::json!({}));
            Ok(Session {
                key: key.clone(),
                last_bot_message: row.last_bot_message,
                pending_command: row.pending_command,
                state_data,
            })
        }
        None => Ok(Session::new(&key.user_id, &key.channel_id)),
    }
}

/// Save a session to the store.
pub async fn save_session(
    store: &Arc<dyn StorageBackend>,
    session: &Session,
) -> Result<(), BotError> {
    let state_str = serde_json::to_string(&session.state_data).unwrap_or_else(|_| "{}".to_owned());

    let row = springtale_store::SessionRow {
        user_id: session.key.user_id.clone(),
        channel_id: session.key.channel_id.clone(),
        last_bot_message: session.last_bot_message.clone(),
        pending_command: session.pending_command.clone(),
        state_data: state_str,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    store.upsert_session(&row).await?;
    Ok(())
}
