use std::sync::Arc;

use springtale_store::StorageBackend;

use crate::error::BotError;

/// User preferences wrapper for the bot runtime.
#[derive(Debug, Clone)]
pub struct UserPrefs {
    pub user_id: String,
    pub timezone: String,
    pub language: String,
    pub notifications_enabled: bool,
}

impl UserPrefs {
    /// Default preferences for a user (notifications off per IPV safety).
    pub fn default_for(user_id: &str) -> Self {
        Self {
            user_id: user_id.into(),
            timezone: "UTC".into(),
            language: "en".into(),
            notifications_enabled: false,
        }
    }
}

/// Load user preferences from the store, or return defaults.
pub async fn load_or_default(
    store: &Arc<dyn StorageBackend>,
    user_id: &str,
) -> Result<UserPrefs, BotError> {
    match store.get_user_prefs(user_id).await? {
        Some(row) => Ok(UserPrefs {
            user_id: row.user_id,
            timezone: row.timezone,
            language: row.language,
            notifications_enabled: row.notifications_enabled,
        }),
        None => Ok(UserPrefs::default_for(user_id)),
    }
}
