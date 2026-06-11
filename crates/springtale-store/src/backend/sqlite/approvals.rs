//! SQLite-backed approval-over-chat state (W2) — pending approvals +
//! tool-loop checkpoints. See `schema/sql/approvals.sql` for the DDL and
//! `schema/approvals.rs` for the row contracts.

use rusqlite::{OptionalExtension, params};

use crate::error::StoreError;
use crate::schema::approvals::{PendingApprovalRow, ToolLoopCheckpointRow};

use super::SqliteBackend;

fn approval_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PendingApprovalRow> {
    Ok(PendingApprovalRow {
        id: row.get(0)?,
        connector_name: row.get(1)?,
        capability_json: row.get(2)?,
        agent_id: row.get(3)?,
        summary: row.get(4)?,
        requested_at: row.get(5)?,
        expires_at: row.get(6)?,
        decision_json: row.get(7)?,
    })
}

const APPROVAL_COLS: &str = "id, connector_name, capability_json, agent_id, summary, requested_at, expires_at, decision_json";

impl SqliteBackend {
    pub(super) async fn insert_pending_approval_impl(
        &self,
        r: PendingApprovalRow,
    ) -> Result<(), StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            conn.execute(
                "INSERT INTO pending_approvals \
                 (id, connector_name, capability_json, agent_id, summary, requested_at, expires_at, decision_json) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    r.id,
                    r.connector_name,
                    r.capability_json,
                    r.agent_id,
                    r.summary,
                    r.requested_at,
                    r.expires_at,
                    r.decision_json,
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(format!("join: {e}")))?
    }

    pub(super) async fn get_pending_approval_impl(
        &self,
        id: String,
    ) -> Result<Option<PendingApprovalRow>, StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let row = conn
                .query_row(
                    &format!("SELECT {APPROVAL_COLS} FROM pending_approvals WHERE id = ?1"),
                    params![id],
                    approval_from_row,
                )
                .optional()?;
            Ok(row)
        })
        .await
        .map_err(|e| StoreError::Database(format!("join: {e}")))?
    }

    pub(super) async fn resolve_pending_approval_impl(
        &self,
        id: String,
        decision_json: String,
    ) -> Result<bool, StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            // Idempotent: only an undecided row accepts a decision.
            let changed = conn.execute(
                "UPDATE pending_approvals SET decision_json = ?2 \
                 WHERE id = ?1 AND decision_json IS NULL",
                params![id, decision_json],
            )?;
            Ok(changed == 1)
        })
        .await
        .map_err(|e| StoreError::Database(format!("join: {e}")))?
    }

    pub(super) async fn list_pending_approvals_impl(
        &self,
        now_ms: i64,
    ) -> Result<Vec<PendingApprovalRow>, StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let mut stmt = conn.prepare(&format!(
                "SELECT {APPROVAL_COLS} FROM pending_approvals \
                 WHERE decision_json IS NULL AND expires_at > ?1 \
                 ORDER BY requested_at ASC"
            ))?;
            let rows = stmt
                .query_map(params![now_ms], approval_from_row)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await
        .map_err(|e| StoreError::Database(format!("join: {e}")))?
    }

    pub(super) async fn expire_pending_approvals_impl(
        &self,
        now_ms: i64,
    ) -> Result<Vec<PendingApprovalRow>, StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let tx = conn.unchecked_transaction()?;
            let expired: Vec<PendingApprovalRow> = {
                let mut stmt = tx.prepare(&format!(
                    "SELECT {APPROVAL_COLS} FROM pending_approvals \
                     WHERE decision_json IS NULL AND expires_at <= ?1"
                ))?;
                stmt.query_map(params![now_ms], approval_from_row)?
                    .collect::<Result<Vec<_>, _>>()?
            };
            // Deny-by-default: stamp a terminal timed-out decision so a
            // restarted daemon never resurrects a stale grant window.
            tx.execute(
                "UPDATE pending_approvals \
                 SET decision_json = json_object('kind','timed_out','timed_out_at', datetime('now')) \
                 WHERE decision_json IS NULL AND expires_at <= ?1",
                params![now_ms],
            )?;
            tx.commit()?;
            Ok(expired)
        })
        .await
        .map_err(|e| StoreError::Database(format!("join: {e}")))?
    }

    pub(super) async fn upsert_tool_loop_checkpoint_impl(
        &self,
        r: ToolLoopCheckpointRow,
    ) -> Result<(), StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            conn.execute(
                "INSERT INTO tool_loop_checkpoints \
                 (session_key, approval_id, origin_connector, origin_channel, messages_json, pending_tool_json, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
                 ON CONFLICT(session_key) DO UPDATE SET \
                   approval_id = excluded.approval_id, \
                   messages_json = excluded.messages_json, \
                   pending_tool_json = excluded.pending_tool_json",
                params![
                    r.session_key,
                    r.approval_id,
                    r.origin_connector,
                    r.origin_channel,
                    r.messages_json,
                    r.pending_tool_json,
                    r.created_at,
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(format!("join: {e}")))?
    }

    pub(super) async fn get_tool_loop_checkpoint_impl(
        &self,
        key: String,
        by_approval: bool,
    ) -> Result<Option<ToolLoopCheckpointRow>, StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let where_col = if by_approval {
                "approval_id"
            } else {
                "session_key"
            };
            let row = conn
                .query_row(
                    &format!(
                        "SELECT session_key, approval_id, origin_connector, origin_channel, \
                                messages_json, pending_tool_json, created_at \
                         FROM tool_loop_checkpoints WHERE {where_col} = ?1"
                    ),
                    params![key],
                    |row| {
                        Ok(ToolLoopCheckpointRow {
                            session_key: row.get(0)?,
                            approval_id: row.get(1)?,
                            origin_connector: row.get(2)?,
                            origin_channel: row.get(3)?,
                            messages_json: row.get(4)?,
                            pending_tool_json: row.get(5)?,
                            created_at: row.get(6)?,
                        })
                    },
                )
                .optional()?;
            Ok(row)
        })
        .await
        .map_err(|e| StoreError::Database(format!("join: {e}")))?
    }

    pub(super) async fn list_tool_loop_checkpoints_impl(
        &self,
    ) -> Result<Vec<ToolLoopCheckpointRow>, StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let mut stmt = conn.prepare(
                "SELECT session_key, approval_id, origin_connector, origin_channel, \
                        messages_json, pending_tool_json, created_at \
                 FROM tool_loop_checkpoints ORDER BY created_at ASC",
            )?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(ToolLoopCheckpointRow {
                        session_key: row.get(0)?,
                        approval_id: row.get(1)?,
                        origin_connector: row.get(2)?,
                        origin_channel: row.get(3)?,
                        messages_json: row.get(4)?,
                        pending_tool_json: row.get(5)?,
                        created_at: row.get(6)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await
        .map_err(|e| StoreError::Database(format!("join: {e}")))?
    }

    pub(super) async fn find_approval_by_summary_impl(
        &self,
        summary: String,
        since_ms: i64,
    ) -> Result<Option<PendingApprovalRow>, StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let row = conn
                .query_row(
                    &format!(
                        "SELECT {APPROVAL_COLS} FROM pending_approvals \
                         WHERE summary = ?1 AND requested_at >= ?2 \
                         ORDER BY requested_at DESC LIMIT 1"
                    ),
                    params![summary, since_ms],
                    approval_from_row,
                )
                .optional()?;
            Ok(row)
        })
        .await
        .map_err(|e| StoreError::Database(format!("join: {e}")))?
    }

    pub(super) async fn delete_tool_loop_checkpoint_impl(
        &self,
        session_key: String,
    ) -> Result<(), StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            conn.execute(
                "DELETE FROM tool_loop_checkpoints WHERE session_key = ?1",
                params![session_key],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(format!("join: {e}")))?
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use crate::backend::trait_::StorageBackend;
    use crate::schema::approvals::{PendingApprovalRow, ToolLoopCheckpointRow};

    fn row(id: &str, expires_at: i64) -> PendingApprovalRow {
        PendingApprovalRow {
            id: id.to_owned(),
            connector_name: "connector-shell".to_owned(),
            capability_json: "\"ShellExec\"".to_owned(),
            agent_id: None,
            summary: "exec: rm -rf ~/tmp/logs".to_owned(),
            requested_at: 1_000,
            expires_at,
            decision_json: None,
        }
    }

    #[tokio::test]
    async fn approval_roundtrip_and_idempotent_resolve() {
        let store = crate::backend::sqlite::SqliteBackend::open_in_memory().unwrap();
        store
            .insert_pending_approval(row("a1", 10_000))
            .await
            .unwrap();

        let pending = store.list_pending_approvals(2_000).await.unwrap();
        assert_eq!(pending.len(), 1);

        // First resolve lands; second is rejected (idempotency).
        assert!(
            store
                .resolve_pending_approval("a1", "{\"kind\":\"approved\"}")
                .await
                .unwrap()
        );
        assert!(
            !store
                .resolve_pending_approval("a1", "{\"kind\":\"denied\"}")
                .await
                .unwrap()
        );

        let got = store.get_pending_approval("a1").await.unwrap().unwrap();
        assert!(got.decision_json.unwrap().contains("approved"));
        assert!(
            store
                .list_pending_approvals(2_000)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn expiry_sweep_denies_and_returns_rows() {
        let store = crate::backend::sqlite::SqliteBackend::open_in_memory().unwrap();
        store
            .insert_pending_approval(row("old", 1_500))
            .await
            .unwrap();
        store
            .insert_pending_approval(row("live", 99_000))
            .await
            .unwrap();

        let expired = store.expire_pending_approvals(2_000).await.unwrap();
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].id, "old");

        // Expired row is now decided (timed_out); live row still pending.
        let old = store.get_pending_approval("old").await.unwrap().unwrap();
        assert!(old.decision_json.unwrap().contains("timed_out"));
        assert_eq!(store.list_pending_approvals(2_000).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn checkpoint_roundtrip() {
        let store = crate::backend::sqlite::SqliteBackend::open_in_memory().unwrap();
        let cp = ToolLoopCheckpointRow {
            session_key: "telegram:123".to_owned(),
            approval_id: Some("a1".to_owned()),
            origin_connector: "connector-telegram".to_owned(),
            origin_channel: "123".to_owned(),
            messages_json: "[]".to_owned(),
            pending_tool_json: "{}".to_owned(),
            created_at: 1_000,
        };
        store.upsert_tool_loop_checkpoint(cp.clone()).await.unwrap();
        // Lookup by session (thread id) AND by approval correlation.
        let by_session = store
            .get_tool_loop_checkpoint("telegram:123")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(by_session.approval_id.as_deref(), Some("a1"));
        let by_approval = store
            .get_checkpoint_by_approval("a1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(by_approval.session_key, "telegram:123");
        store
            .delete_tool_loop_checkpoint("telegram:123")
            .await
            .unwrap();
        assert!(
            store
                .get_tool_loop_checkpoint("telegram:123")
                .await
                .unwrap()
                .is_none()
        );
    }
}
