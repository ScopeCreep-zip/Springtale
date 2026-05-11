use chrono::Utc;
use rusqlite::params;

use crate::error::StoreError;
use crate::schema::safety::SafetyConfigRow;

use super::SqliteBackend;

impl SqliteBackend {
    pub(super) async fn get_safety_config_impl(
        &self,
    ) -> Result<Option<SafetyConfigRow>, StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            // G5d — read the disguise columns alongside the legacy
            // safety fields. The columns were added by migration 012;
            // pre-012 databases get the defaults via the column
            // `DEFAULT` clauses in the migration SQL.
            let result = conn.query_row(
                "SELECT window_title, auto_lock_minutes, content_protected, quick_hide_shortcut,
                        disguise_app_name, disguise_icon_id, disguise_active, panic_tap_count,
                        updated_at
                 FROM safety_config WHERE id = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, bool>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, bool>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, String>(8)?,
                    ))
                },
            );

            match result {
                Ok((title, minutes, protected, shortcut, app_name, icon_id, active, panic_taps, updated)) => {
                    let updated_at = chrono::DateTime::parse_from_rfc3339(&updated)
                        .map(|dt| dt.with_timezone(&Utc))
                        .map_err(|e| StoreError::Serialization(e.to_string()))?;
                    Ok(Some(SafetyConfigRow {
                        window_title: title,
                        auto_lock_minutes: minutes as u32,
                        content_protected: protected,
                        quick_hide_shortcut: shortcut,
                        disguise_app_name: app_name,
                        disguise_icon_id: icon_id,
                        disguise_active: active,
                        panic_tap_count: panic_taps as u32,
                        updated_at,
                    }))
                }
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }

    pub(super) async fn upsert_safety_config_impl(
        &self,
        config: &SafetyConfigRow,
    ) -> Result<(), StoreError> {
        let conn = self.conn.clone();
        let config = config.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            // G5d — write all safety fields including disguise columns.
            // The ON CONFLICT clause must enumerate every column so an
            // existing row gets a full refresh, not a partial.
            conn.execute(
                "INSERT INTO safety_config
                    (id, window_title, auto_lock_minutes, content_protected, quick_hide_shortcut,
                     disguise_app_name, disguise_icon_id, disguise_active, panic_tap_count,
                     updated_at)
                 VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(id) DO UPDATE SET
                    window_title = excluded.window_title,
                    auto_lock_minutes = excluded.auto_lock_minutes,
                    content_protected = excluded.content_protected,
                    quick_hide_shortcut = excluded.quick_hide_shortcut,
                    disguise_app_name = excluded.disguise_app_name,
                    disguise_icon_id = excluded.disguise_icon_id,
                    disguise_active = excluded.disguise_active,
                    panic_tap_count = excluded.panic_tap_count,
                    updated_at = excluded.updated_at",
                params![
                    config.window_title,
                    config.auto_lock_minutes as i64,
                    config.content_protected,
                    config.quick_hide_shortcut,
                    config.disguise_app_name,
                    config.disguise_icon_id,
                    config.disguise_active,
                    config.panic_tap_count as i64,
                    config.updated_at.to_rfc3339(),
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }
}
