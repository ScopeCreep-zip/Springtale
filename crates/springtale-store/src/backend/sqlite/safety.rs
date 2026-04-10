use chrono::Utc;
use rusqlite::params;

use crate::error::StoreError;
use crate::schema::safety::SafetyConfigRow;

use super::SqliteBackend;

impl SqliteBackend {
    pub(super) async fn get_safety_config_impl(
        &self,
    ) -> Result<Option<SafetyConfigRow>, StoreError> {
        let conn = self.conn.lock().await;
        let result = conn.query_row(
            "SELECT window_title, auto_lock_minutes, content_protected, quick_hide_shortcut, updated_at
             FROM safety_config WHERE id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, bool>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        );

        match result {
            Ok((title, minutes, protected, shortcut, updated)) => {
                let updated_at = chrono::DateTime::parse_from_rfc3339(&updated)
                    .map(|dt| dt.with_timezone(&Utc))
                    .map_err(|e| StoreError::Serialization(e.to_string()))?;
                Ok(Some(SafetyConfigRow {
                    window_title: title,
                    auto_lock_minutes: minutes as u32,
                    content_protected: protected,
                    quick_hide_shortcut: shortcut,
                    updated_at,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub(super) async fn upsert_safety_config_impl(
        &self,
        config: &SafetyConfigRow,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO safety_config (id, window_title, auto_lock_minutes, content_protected, quick_hide_shortcut, updated_at)
             VALUES (1, ?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
                window_title = excluded.window_title,
                auto_lock_minutes = excluded.auto_lock_minutes,
                content_protected = excluded.content_protected,
                quick_hide_shortcut = excluded.quick_hide_shortcut,
                updated_at = excluded.updated_at",
            params![
                config.window_title,
                config.auto_lock_minutes as i64,
                config.content_protected,
                config.quick_hide_shortcut,
                config.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }
}
