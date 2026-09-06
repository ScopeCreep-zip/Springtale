//! SQLite-backed long-lived API tokens (plan 6.6). Row contract in
//! `schema/api_tokens.rs`, DDL in `schema/sql/api_tokens.sql`.

use rusqlite::{OptionalExtension, params};

use crate::error::StoreError;
use crate::schema::api_tokens::ApiTokenRow;

use super::SqliteBackend;

const TOKEN_COLS: &str = "id, name, token_hash, created_at, last_used";

fn token_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ApiTokenRow> {
    Ok(ApiTokenRow {
        id: row.get(0)?,
        name: row.get(1)?,
        token_hash: row.get(2)?,
        created_at: row.get(3)?,
        last_used: row.get(4)?,
    })
}

impl SqliteBackend {
    pub(super) async fn insert_api_token_impl(&self, r: ApiTokenRow) -> Result<(), StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            conn.execute(
                "INSERT INTO api_tokens (id, name, token_hash, created_at, last_used) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![r.id, r.name, r.token_hash, r.created_at, r.last_used],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(format!("join: {e}")))?
    }

    pub(super) async fn list_api_tokens_impl(&self) -> Result<Vec<ApiTokenRow>, StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let mut stmt = conn.prepare(&format!(
                "SELECT {TOKEN_COLS} FROM api_tokens ORDER BY created_at DESC"
            ))?;
            let rows = stmt.query_map([], token_from_row)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
        .await
        .map_err(|e| StoreError::Database(format!("join: {e}")))?
    }

    pub(super) async fn find_api_token_by_hash_impl(
        &self,
        hash: Vec<u8>,
    ) -> Result<Option<ApiTokenRow>, StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let row = conn
                .query_row(
                    &format!("SELECT {TOKEN_COLS} FROM api_tokens WHERE token_hash = ?1"),
                    params![hash],
                    token_from_row,
                )
                .optional()?;
            Ok(row)
        })
        .await
        .map_err(|e| StoreError::Database(format!("join: {e}")))?
    }

    pub(super) async fn touch_api_token_impl(
        &self,
        id: String,
        now_ms: i64,
    ) -> Result<(), StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            conn.execute(
                "UPDATE api_tokens SET last_used = ?2 WHERE id = ?1",
                params![id, now_ms],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(format!("join: {e}")))?
    }

    pub(super) async fn delete_api_token_impl(&self, id: String) -> Result<bool, StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let n = conn.execute("DELETE FROM api_tokens WHERE id = ?1", params![id])?;
            Ok(n > 0)
        })
        .await
        .map_err(|e| StoreError::Database(format!("join: {e}")))?
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    async fn store() -> SqliteBackend {
        SqliteBackend::open_in_memory().unwrap()
    }

    fn row(id: &str, hash: u8) -> ApiTokenRow {
        ApiTokenRow {
            id: id.to_owned(),
            name: format!("token-{id}"),
            token_hash: vec![hash; 32],
            created_at: 1_000,
            last_used: None,
        }
    }

    #[tokio::test]
    async fn test_find_api_token_by_hash_unknown_returns_none() {
        let s = store().await;
        s.insert_api_token_impl(row("a", 1)).await.unwrap();
        assert!(
            s.find_api_token_by_hash_impl(vec![9; 32])
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn test_delete_api_token_revokes_immediately() {
        let s = store().await;
        s.insert_api_token_impl(row("a", 1)).await.unwrap();
        assert!(
            s.find_api_token_by_hash_impl(vec![1; 32])
                .await
                .unwrap()
                .is_some()
        );
        assert!(s.delete_api_token_impl("a".to_owned()).await.unwrap());
        assert!(
            s.find_api_token_by_hash_impl(vec![1; 32])
                .await
                .unwrap()
                .is_none()
        );
        assert!(!s.delete_api_token_impl("a".to_owned()).await.unwrap());
    }

    #[tokio::test]
    async fn test_touch_api_token_records_last_used() {
        let s = store().await;
        s.insert_api_token_impl(row("a", 1)).await.unwrap();
        s.touch_api_token_impl("a".to_owned(), 5_000).await.unwrap();
        let found = s
            .find_api_token_by_hash_impl(vec![1; 32])
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.last_used, Some(5_000));
        assert_eq!(s.list_api_tokens_impl().await.unwrap().len(), 1);
    }
}
