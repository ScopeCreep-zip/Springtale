//! SQLite-backed AI token usage (Phase-7 audit Finding D).
//!
//! Per-bot daily quota persistence. All UPSERTs run under the
//! connection mutex so reserve + commit operations are
//! serialisable — concurrent dispatches against the same bot race
//! cleanly through SQLite's WAL.

use chrono::Utc;
use rusqlite::params;

use crate::backend::AiTokenReserveOutcome;
use crate::error::StoreError;

use super::SqliteBackend;

impl SqliteBackend {
    pub(super) async fn ai_token_usage_get_impl(
        &self,
        agent_id: &str,
        day_ymd: u32,
    ) -> Result<u64, StoreError> {
        let conn = self.conn.clone();
        let agent_id = agent_id.to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let tokens: Option<i64> = conn
                .query_row(
                    "SELECT tokens_used FROM ai_token_usage WHERE agent_id = ?1 AND day_ymd = ?2",
                    params![agent_id, day_ymd as i64],
                    |row| row.get(0),
                )
                .ok();
            Ok(tokens.unwrap_or(0).max(0) as u64)
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }

    pub(super) async fn ai_token_usage_set_impl(
        &self,
        agent_id: &str,
        day_ymd: u32,
        tokens_used: u64,
    ) -> Result<(), StoreError> {
        let conn = self.conn.clone();
        let agent_id = agent_id.to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            conn.execute(
                "INSERT INTO ai_token_usage (agent_id, day_ymd, tokens_used, updated_at) \
                 VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT(agent_id, day_ymd) DO UPDATE SET \
                   tokens_used = excluded.tokens_used, \
                   updated_at  = excluded.updated_at",
                params![
                    agent_id,
                    day_ymd as i64,
                    tokens_used as i64,
                    Utc::now().to_rfc3339(),
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }

    /// Atomic commit adjustment: replace a pessimistic reservation
    /// with actual usage in a single SQL `UPDATE` so two concurrent
    /// commits for the same `(agent_id, day_ymd)` can't race a
    /// stale `tokens_used` into the write. Equivalent to
    /// `tokens_used = max(0, tokens_used - prior + actual)`. If the
    /// row doesn't exist (day rollover between reserve and commit),
    /// inserts `actual_tokens` so the new day's counter reflects the
    /// real usage we just observed.
    pub(super) async fn ai_token_usage_commit_impl(
        &self,
        agent_id: &str,
        day_ymd: u32,
        prior_reservation: u64,
        actual_tokens: u64,
    ) -> Result<(), StoreError> {
        let conn = self.conn.clone();
        let agent_id = agent_id.to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            // SQLite's MAX is row-aggregate; for two scalars use the
            // IIF(a>b, a, b) idiom. `tokens_used - prior + actual`
            // is computed inside SQLite so the read and write happen
            // in one statement under the connection mutex.
            conn.execute(
                "INSERT INTO ai_token_usage (agent_id, day_ymd, tokens_used, updated_at) \
                 VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT(agent_id, day_ymd) DO UPDATE SET \
                   tokens_used = IIF( \
                     (tokens_used - ?5 + ?3) > 0, \
                     (tokens_used - ?5 + ?3), \
                     0 \
                   ), \
                   updated_at = excluded.updated_at",
                params![
                    agent_id,
                    day_ymd as i64,
                    actual_tokens as i64,
                    Utc::now().to_rfc3339(),
                    prior_reservation as i64,
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }

    /// Atomic reserve: runs the read-compare-write under a single
    /// connection lock so concurrent reserves can't both observe the
    /// same `used` value and double-reserve past the cap.
    pub(super) async fn ai_token_usage_reserve_impl(
        &self,
        agent_id: &str,
        day_ymd: u32,
        requested: u64,
        limit: Option<u64>,
    ) -> Result<AiTokenReserveOutcome, StoreError> {
        let conn = self.conn.clone();
        let agent_id = agent_id.to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let used: u64 = conn
                .query_row(
                    "SELECT tokens_used FROM ai_token_usage \
                     WHERE agent_id = ?1 AND day_ymd = ?2",
                    params![agent_id, day_ymd as i64],
                    |row| row.get::<_, i64>(0),
                )
                .ok()
                .map(|v| v.max(0) as u64)
                .unwrap_or(0);
            let new_total = used.saturating_add(requested);
            if let Some(cap) = limit
                && new_total > cap
            {
                return Ok(AiTokenReserveOutcome::Denied { used, limit: cap });
            }
            conn.execute(
                "INSERT INTO ai_token_usage (agent_id, day_ymd, tokens_used, updated_at) \
                 VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT(agent_id, day_ymd) DO UPDATE SET \
                   tokens_used = excluded.tokens_used, \
                   updated_at  = excluded.updated_at",
                params![
                    agent_id,
                    day_ymd as i64,
                    new_total as i64,
                    Utc::now().to_rfc3339(),
                ],
            )?;
            Ok(AiTokenReserveOutcome::Reserved {
                total_after: new_total,
            })
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }
}
