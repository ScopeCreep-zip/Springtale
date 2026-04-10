use chrono::Utc;
use rusqlite::params;

use crate::error::StoreError;
use crate::schema::bot::{SessionRow, UserPrefsRow};

use super::SqliteBackend;

impl SqliteBackend {
    pub(super) async fn upsert_session_impl(
        &self,
        session: &SessionRow,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO bot_sessions (user_id, channel_id, last_bot_message, pending_command, state_data, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(user_id, channel_id) DO UPDATE SET
                last_bot_message = excluded.last_bot_message,
                pending_command = excluded.pending_command,
                state_data = excluded.state_data,
                updated_at = excluded.updated_at",
            params![
                session.user_id,
                session.channel_id,
                session.last_bot_message,
                session.pending_command,
                session.state_data,
                session.created_at.to_rfc3339(),
                session.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub(super) async fn get_session_impl(
        &self,
        user_id: &str,
        channel_id: &str,
    ) -> Result<Option<SessionRow>, StoreError> {
        let conn = self.conn.lock().await;
        let result = conn.query_row(
            "SELECT user_id, channel_id, last_bot_message, pending_command, state_data, created_at, updated_at
             FROM bot_sessions WHERE user_id = ?1 AND channel_id = ?2",
            params![user_id, channel_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        );

        match result {
            Ok((uid, cid, last_msg, pending, state, created, updated)) => {
                let created_at = chrono::DateTime::parse_from_rfc3339(&created)
                    .map(|dt| dt.with_timezone(&Utc))
                    .map_err(|e| StoreError::Serialization(e.to_string()))?;
                let updated_at = chrono::DateTime::parse_from_rfc3339(&updated)
                    .map(|dt| dt.with_timezone(&Utc))
                    .map_err(|e| StoreError::Serialization(e.to_string()))?;
                Ok(Some(SessionRow {
                    user_id: uid,
                    channel_id: cid,
                    last_bot_message: last_msg,
                    pending_command: pending,
                    state_data: state,
                    created_at,
                    updated_at,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub(super) async fn delete_session_impl(
        &self,
        user_id: &str,
        channel_id: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "DELETE FROM bot_sessions WHERE user_id = ?1 AND channel_id = ?2",
            params![user_id, channel_id],
        )?;
        Ok(())
    }

    pub(super) async fn list_sessions_impl(&self) -> Result<Vec<SessionRow>, StoreError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT user_id, channel_id, last_bot_message, pending_command, \
             state_data, created_at, updated_at \
             FROM bot_sessions ORDER BY updated_at DESC",
        )?;
        let tuples: Vec<(
            String,
            String,
            Option<String>,
            Option<String>,
            String,
            String,
            String,
        )> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut sessions = Vec::with_capacity(tuples.len());
        for (uid, cid, last_msg, pending, state, created, updated) in tuples {
            let created_at = chrono::DateTime::parse_from_rfc3339(&created)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| StoreError::Serialization(e.to_string()))?;
            let updated_at = chrono::DateTime::parse_from_rfc3339(&updated)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| StoreError::Serialization(e.to_string()))?;
            sessions.push(SessionRow {
                user_id: uid,
                channel_id: cid,
                last_bot_message: last_msg,
                pending_command: pending,
                state_data: state,
                created_at,
                updated_at,
            });
        }
        Ok(sessions)
    }

    pub(super) async fn upsert_user_prefs_impl(
        &self,
        prefs: &UserPrefsRow,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO user_prefs (user_id, timezone, language, notifications_enabled, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(user_id) DO UPDATE SET
                timezone = excluded.timezone,
                language = excluded.language,
                notifications_enabled = excluded.notifications_enabled,
                updated_at = excluded.updated_at",
            params![
                prefs.user_id,
                prefs.timezone,
                prefs.language,
                prefs.notifications_enabled,
                prefs.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub(super) async fn get_user_prefs_impl(
        &self,
        user_id: &str,
    ) -> Result<Option<UserPrefsRow>, StoreError> {
        let conn = self.conn.lock().await;
        let result = conn.query_row(
            "SELECT user_id, timezone, language, notifications_enabled, updated_at
             FROM user_prefs WHERE user_id = ?1",
            params![user_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, bool>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        );

        match result {
            Ok((uid, tz, lang, notif, updated)) => {
                let updated_at = chrono::DateTime::parse_from_rfc3339(&updated)
                    .map(|dt| dt.with_timezone(&Utc))
                    .map_err(|e| StoreError::Serialization(e.to_string()))?;
                Ok(Some(UserPrefsRow {
                    user_id: uid,
                    timezone: tz,
                    language: lang,
                    notifications_enabled: notif,
                    updated_at,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}
