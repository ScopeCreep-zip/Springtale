use chrono::Utc;
use rusqlite::params;

use crate::error::StoreError;

use super::SqliteBackend;

impl SqliteBackend {
    pub(super) async fn upsert_alias_impl(
        &self,
        alias: &str,
        target: &str,
        created_by: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO bot_aliases (alias, target, created_by, created_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(alias) DO UPDATE SET
                target = excluded.target,
                created_by = excluded.created_by,
                created_at = excluded.created_at",
            params![alias, target, created_by, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub(super) async fn list_aliases_impl(&self) -> Result<Vec<(String, String)>, StoreError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare("SELECT alias, target FROM bot_aliases ORDER BY alias")?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<(String, String)>, _>>()?;
        Ok(rows)
    }

    pub(super) async fn delete_alias_impl(&self, alias: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        conn.execute("DELETE FROM bot_aliases WHERE alias = ?1", params![alias])?;
        Ok(())
    }
}
