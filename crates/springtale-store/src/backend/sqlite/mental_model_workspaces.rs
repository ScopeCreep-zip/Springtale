//! SQLite-backed external-workspace directory — D1.
//!
//! Per-key CRUD against `mental_model_workspaces`. The gossip-delta
//! merge happens in the cooperation crate before we get here; this
//! module is just storage.

use rusqlite::params;

use crate::error::StoreError;
use crate::schema::mental_model::MentalModelWorkspaceRow;

use super::SqliteBackend;

impl SqliteBackend {
    pub(super) async fn mental_model_workspace_upsert_impl(
        &self,
        formation_id: String,
        row: MentalModelWorkspaceRow,
    ) -> Result<(), StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            conn.execute(
                "INSERT INTO mental_model_workspaces (
                    formation_id, workspace_key, connector_name, display_name,
                    kind, metadata_json, first_seen_at, last_seen_at, provenance_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT (formation_id, workspace_key) DO UPDATE SET
                    connector_name  = excluded.connector_name,
                    display_name    = excluded.display_name,
                    kind            = excluded.kind,
                    metadata_json   = excluded.metadata_json,
                    first_seen_at   = MIN(mental_model_workspaces.first_seen_at, excluded.first_seen_at),
                    last_seen_at    = excluded.last_seen_at,
                    provenance_json = excluded.provenance_json",
                params![
                    formation_id,
                    row.workspace_key,
                    row.connector_name,
                    row.display_name,
                    row.kind,
                    row.metadata_json,
                    row.first_seen_at_unix_ms,
                    row.last_seen_at_unix_ms,
                    row.provenance_json,
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }

    pub(super) async fn mental_model_workspaces_for_formation_impl(
        &self,
        formation_id: String,
        connector_filter: Option<String>,
    ) -> Result<Vec<MentalModelWorkspaceRow>, StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let mut rows = Vec::new();
            match connector_filter {
                Some(filter) => {
                    let mut stmt = conn.prepare(
                        "SELECT workspace_key, connector_name, display_name, kind,
                                metadata_json, first_seen_at, last_seen_at, provenance_json
                           FROM mental_model_workspaces
                          WHERE formation_id = ?1 AND connector_name = ?2
                          ORDER BY last_seen_at DESC",
                    )?;
                    let iter = stmt.query_map(params![formation_id, filter], |row| {
                        Ok(MentalModelWorkspaceRow {
                            workspace_key: row.get(0)?,
                            connector_name: row.get(1)?,
                            display_name: row.get(2)?,
                            kind: row.get(3)?,
                            metadata_json: row.get(4)?,
                            first_seen_at_unix_ms: row.get(5)?,
                            last_seen_at_unix_ms: row.get(6)?,
                            provenance_json: row.get(7)?,
                        })
                    })?;
                    for r in iter {
                        rows.push(r?);
                    }
                }
                None => {
                    let mut stmt = conn.prepare(
                        "SELECT workspace_key, connector_name, display_name, kind,
                                metadata_json, first_seen_at, last_seen_at, provenance_json
                           FROM mental_model_workspaces
                          WHERE formation_id = ?1
                          ORDER BY last_seen_at DESC",
                    )?;
                    let iter = stmt.query_map(params![formation_id], |row| {
                        Ok(MentalModelWorkspaceRow {
                            workspace_key: row.get(0)?,
                            connector_name: row.get(1)?,
                            display_name: row.get(2)?,
                            kind: row.get(3)?,
                            metadata_json: row.get(4)?,
                            first_seen_at_unix_ms: row.get(5)?,
                            last_seen_at_unix_ms: row.get(6)?,
                            provenance_json: row.get(7)?,
                        })
                    })?;
                    for r in iter {
                        rows.push(r?);
                    }
                }
            };
            Ok(rows)
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }

    pub(super) async fn mental_model_workspace_delete_impl(
        &self,
        formation_id: String,
        workspace_key: String,
    ) -> Result<(), StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            conn.execute(
                "DELETE FROM mental_model_workspaces
                  WHERE formation_id = ?1 AND workspace_key = ?2",
                params![formation_id, workspace_key],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }

    pub(super) async fn mental_model_workspace_touch_impl(
        &self,
        formation_id: String,
        workspace_key: String,
        now_unix_ms: i64,
    ) -> Result<(), StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            conn.execute(
                "UPDATE mental_model_workspaces
                    SET last_seen_at = ?3
                  WHERE formation_id = ?1 AND workspace_key = ?2",
                params![formation_id, workspace_key, now_unix_ms],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::backend::trait_::StorageBackend;
    use crate::schema::formations::FormationRow;
    use crate::SqliteBackend;
    use std::sync::Arc;

    fn sample_row(key: &str, seen_at: i64) -> MentalModelWorkspaceRow {
        MentalModelWorkspaceRow {
            workspace_key: key.into(),
            connector_name: "connector-telegram".into(),
            display_name: "Test Chat".into(),
            kind: "user".into(),
            metadata_json: None,
            first_seen_at_unix_ms: seen_at,
            last_seen_at_unix_ms: seen_at,
            provenance_json: r#"{"manual_entry":{"entered_by":"00000000-0000-0000-0000-000000000000"}}"#.into(),
        }
    }

    async fn setup_with_formation() -> (Arc<SqliteBackend>, String) {
        let store = Arc::new(SqliteBackend::open_in_memory().unwrap());
        let formation_id = "formA".to_owned();
        // Insert minimal formation row so the FK passes.
        let now = chrono::Utc::now();
        store
            .insert_formation(&FormationRow {
                id: formation_id.clone(),
                name: "F".into(),
                intent: "reconnoiter".into(),
                status: "active".into(),
                created_at: now,
                updated_at: now,
            })
            .await
            .unwrap();
        (store, formation_id)
    }

    #[tokio::test]
    async fn upsert_then_list_returns_row() {
        let (store, formation_id) = setup_with_formation().await;
        let row = sample_row("telegram://chat/12345", 1_000);
        store
            .mental_model_workspace_upsert(&formation_id, &row)
            .await
            .unwrap();
        let rows = store
            .mental_model_workspaces_for_formation(&formation_id, None)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].workspace_key, "telegram://chat/12345");
    }

    #[tokio::test]
    async fn upsert_conflict_preserves_earliest_first_seen() {
        let (store, formation_id) = setup_with_formation().await;
        let mut first = sample_row("telegram://chat/1", 1_000);
        first.first_seen_at_unix_ms = 1_000;
        store
            .mental_model_workspace_upsert(&formation_id, &first)
            .await
            .unwrap();

        let mut later = sample_row("telegram://chat/1", 2_000);
        later.first_seen_at_unix_ms = 2_000; // newer first_seen_at — should be ignored
        store
            .mental_model_workspace_upsert(&formation_id, &later)
            .await
            .unwrap();

        let rows = store
            .mental_model_workspaces_for_formation(&formation_id, None)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].first_seen_at_unix_ms, 1_000);
        assert_eq!(rows[0].last_seen_at_unix_ms, 2_000);
    }

    #[tokio::test]
    async fn list_filters_by_connector() {
        let (store, formation_id) = setup_with_formation().await;
        let mut tg = sample_row("telegram://chat/1", 1_000);
        tg.connector_name = "connector-telegram".into();
        let mut dc = sample_row("discord://channel/C1", 1_000);
        dc.connector_name = "connector-discord".into();
        store
            .mental_model_workspace_upsert(&formation_id, &tg)
            .await
            .unwrap();
        store
            .mental_model_workspace_upsert(&formation_id, &dc)
            .await
            .unwrap();

        let tg_only = store
            .mental_model_workspaces_for_formation(
                &formation_id,
                Some("connector-telegram"),
            )
            .await
            .unwrap();
        assert_eq!(tg_only.len(), 1);
        assert_eq!(tg_only[0].connector_name, "connector-telegram");
    }

    #[tokio::test]
    async fn list_orders_newest_first() {
        let (store, formation_id) = setup_with_formation().await;
        for (key, t) in [
            ("telegram://chat/oldest", 1_000),
            ("telegram://chat/newest", 3_000),
            ("telegram://chat/middle", 2_000),
        ] {
            store
                .mental_model_workspace_upsert(&formation_id, &sample_row(key, t))
                .await
                .unwrap();
        }
        let rows = store
            .mental_model_workspaces_for_formation(&formation_id, None)
            .await
            .unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].workspace_key, "telegram://chat/newest");
        assert_eq!(rows[2].workspace_key, "telegram://chat/oldest");
    }

    #[tokio::test]
    async fn delete_removes_one_row() {
        let (store, formation_id) = setup_with_formation().await;
        store
            .mental_model_workspace_upsert(&formation_id, &sample_row("telegram://chat/A", 1_000))
            .await
            .unwrap();
        store
            .mental_model_workspace_upsert(&formation_id, &sample_row("telegram://chat/B", 1_000))
            .await
            .unwrap();
        store
            .mental_model_workspace_delete(&formation_id, "telegram://chat/A")
            .await
            .unwrap();
        let rows = store
            .mental_model_workspaces_for_formation(&formation_id, None)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].workspace_key, "telegram://chat/B");
    }

    #[tokio::test]
    async fn touch_updates_last_seen_only() {
        let (store, formation_id) = setup_with_formation().await;
        store
            .mental_model_workspace_upsert(&formation_id, &sample_row("telegram://chat/A", 1_000))
            .await
            .unwrap();
        store
            .mental_model_workspace_touch(&formation_id, "telegram://chat/A", 5_000)
            .await
            .unwrap();
        let rows = store
            .mental_model_workspaces_for_formation(&formation_id, None)
            .await
            .unwrap();
        assert_eq!(rows[0].first_seen_at_unix_ms, 1_000); // unchanged
        assert_eq!(rows[0].last_seen_at_unix_ms, 5_000);
    }

    #[tokio::test]
    async fn formation_delete_cascades_workspaces() {
        let (store, formation_id) = setup_with_formation().await;
        store
            .mental_model_workspace_upsert(&formation_id, &sample_row("telegram://chat/A", 1_000))
            .await
            .unwrap();
        store.delete_formation(&formation_id).await.unwrap();
        let rows = store
            .mental_model_workspaces_for_formation(&formation_id, None)
            .await
            .unwrap();
        assert!(rows.is_empty(), "workspaces should cascade-delete with formation");
    }
}
