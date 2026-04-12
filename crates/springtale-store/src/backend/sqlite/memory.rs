use chrono::Utc;
use rusqlite::params;

use crate::error::StoreError;
use crate::schema::bot::MemoryRow;

use super::SqliteBackend;

impl SqliteBackend {
    pub(super) async fn insert_memory_impl(&self, entry: &MemoryRow) -> Result<(), StoreError> {
        let conn = self.conn.clone();
        let entry = entry.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            conn.execute(
                "INSERT INTO bot_memory (id, user_id, channel_id, category, schema_version, author, source,
                 content_encrypted, nonce, content_hash, parent_id, trust_score, created_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    entry.id,
                    entry.user_id,
                    entry.channel_id,
                    entry.category,
                    entry.schema_version,
                    entry.author,
                    entry.source,
                    entry.content_encrypted,
                    entry.nonce,
                    entry.content_hash,
                    entry.parent_id,
                    entry.trust_score,
                    entry.created_at.to_rfc3339(),
                    entry.expires_at.as_ref().map(|t| t.to_rfc3339()),
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }

    pub(super) async fn get_memory_impl(
        &self,
        user_id: &str,
        channel_id: &str,
        limit: usize,
    ) -> Result<Vec<MemoryRow>, StoreError> {
        let conn = self.conn.clone();
        let user_id = user_id.to_owned();
        let channel_id = channel_id.to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let mut stmt = conn.prepare(
                "SELECT id, user_id, channel_id, category, schema_version, author, source,
                        content_encrypted, nonce, content_hash, parent_id, trust_score,
                        created_at, expires_at
                 FROM bot_memory
                 WHERE user_id = ?1 AND channel_id = ?2
                 ORDER BY created_at DESC
                 LIMIT ?3",
            )?;

            let rows = stmt
                .query_map(params![user_id, channel_id, limit as i64], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, Vec<u8>>(7)?,
                        row.get::<_, Vec<u8>>(8)?,
                        row.get::<_, Option<String>>(9)?,
                        row.get::<_, Option<String>>(10)?,
                        row.get::<_, f64>(11)?,
                        row.get::<_, String>(12)?,
                        row.get::<_, Option<String>>(13)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;

            let mut entries = Vec::with_capacity(rows.len());
            for r in rows {
                let created_at = chrono::DateTime::parse_from_rfc3339(&r.12)
                    .map(|dt| dt.with_timezone(&Utc))
                    .map_err(|e| StoreError::Serialization(e.to_string()))?;
                let expires_at =
                    r.13.as_ref()
                        .map(|s| {
                            chrono::DateTime::parse_from_rfc3339(s)
                                .map(|dt| dt.with_timezone(&Utc))
                                .map_err(|e| StoreError::Serialization(e.to_string()))
                        })
                        .transpose()?;

                entries.push(MemoryRow {
                    id: r.0,
                    user_id: r.1,
                    channel_id: r.2,
                    category: r.3,
                    schema_version: r.4,
                    author: r.5,
                    source: r.6,
                    content_encrypted: r.7,
                    nonce: r.8,
                    content_hash: r.9,
                    parent_id: r.10,
                    trust_score: r.11,
                    created_at,
                    expires_at,
                });
            }
            Ok(entries)
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }

    pub(super) async fn delete_memory_impl(
        &self,
        user_id: &str,
        channel_id: &str,
    ) -> Result<u64, StoreError> {
        let conn = self.conn.clone();
        let user_id = user_id.to_owned();
        let channel_id = channel_id.to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let deleted = conn.execute(
                "DELETE FROM bot_memory WHERE user_id = ?1 AND channel_id = ?2",
                params![user_id, channel_id],
            )?;
            Ok(deleted as u64)
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }

    pub(super) async fn compact_memory_impl(
        &self,
        user_id: &str,
        channel_id: &str,
        max_entries: usize,
    ) -> Result<u64, StoreError> {
        let conn = self.conn.clone();
        let user_id = user_id.to_owned();
        let channel_id = channel_id.to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            // Delete the oldest entries beyond max_entries.
            // Keep the newest max_entries rows by deleting those NOT IN the top N.
            let deleted = conn.execute(
                "DELETE FROM bot_memory
                 WHERE user_id = ?1 AND channel_id = ?2
                   AND id NOT IN (
                       SELECT id FROM bot_memory
                       WHERE user_id = ?1 AND channel_id = ?2
                       ORDER BY created_at DESC, id DESC
                       LIMIT ?3
                   )",
                params![user_id, channel_id, max_entries as i64],
            )?;
            Ok(deleted as u64)
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }
}
