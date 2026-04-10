use crate::error::StoreError;

use super::SqliteBackend;

impl SqliteBackend {
    pub(super) async fn get_config_impl(&self, key: &str) -> Result<Option<String>, StoreError> {
        let conn = self.conn.lock().await;
        let result = conn.query_row(
            "SELECT value_json FROM config_store WHERE key = ?1",
            rusqlite::params![key],
            |row| row.get::<_, String>(0),
        );
        match result {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StoreError::Database(format!("get_config: {e}"))),
        }
    }

    pub(super) async fn set_config_impl(
        &self,
        key: &str,
        value_json: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO config_store (key, value_json, updated_at) VALUES (?1, ?2, datetime('now'))
             ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
            rusqlite::params![key, value_json],
        )
        .map_err(|e| StoreError::Database(format!("set_config: {e}")))?;
        Ok(())
    }

    pub(super) async fn list_config_impl(&self) -> Result<Vec<(String, String)>, StoreError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn
            .prepare("SELECT key, value_json FROM config_store ORDER BY key")
            .map_err(|e| StoreError::Database(format!("list_config prepare: {e}")))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| StoreError::Database(format!("list_config query: {e}")))?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(row.map_err(|e| StoreError::Database(format!("list_config row: {e}")))?);
        }
        Ok(entries)
    }

    pub(super) async fn delete_config_impl(&self, key: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "DELETE FROM config_store WHERE key = ?1",
            rusqlite::params![key],
        )
        .map_err(|e| StoreError::Database(format!("delete_config: {e}")))?;
        Ok(())
    }
}
