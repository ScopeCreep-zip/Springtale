//! SQLite-backed executions log — chain lifecycle recording with
//! privacy defaults (sizes-only, 14-day retention, opt-in content
//! via separate blob refs).
//!
//! All four trait methods (`record_execution_start`,
//! `record_execution_finish`, `record_execution_step`,
//! `list_executions`, `get_execution_steps`, `vacuum_executions`)
//! land here. Privacy invariant: rows never contain content —
//! only sizes and enum-typed error tags. The bot-configurable
//! content retention path (Phase C) populates `*_blob_ref` to
//! point at a separate KV store; we never inline.

use rusqlite::params;

use crate::error::StoreError;
use crate::schema::executions::{
    ExecutionFilter, ExecutionMode, ExecutionRow, ExecutionStatus, ExecutionStepRow,
    ExecutionSummary, StepStatus,
};
// `MomentumTag` is only referenced from the test fixtures below; pulled
// in conditionally so the lib-target build doesn't flag it as unused
// after the FromStr refactor moved the prod-path callers to `str::parse`.
#[cfg(test)]
use crate::schema::executions::MomentumTag;

use super::SqliteBackend;

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 500;

impl SqliteBackend {
    pub(super) async fn record_execution_start_impl(
        &self,
        exec: ExecutionRow,
    ) -> Result<(), StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            conn.execute(
                "INSERT INTO executions (
                    id, bot_id, formation_id, rule_id, recipe_id,
                    started_at, finished_at, mode, status, momentum,
                    trigger_summary, error_kind, duration_ms,
                    retention_until, retry_of
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?5,
                    ?6, ?7, ?8, ?9, ?10,
                    ?11, ?12, ?13, ?14, ?15
                 )",
                params![
                    exec.id,
                    exec.bot_id,
                    exec.formation_id,
                    exec.rule_id,
                    exec.recipe_id,
                    exec.started_at,
                    exec.finished_at,
                    exec.mode.as_str(),
                    exec.status.as_str(),
                    exec.momentum.as_ref().map(|m| m.as_str()),
                    exec.trigger_summary,
                    exec.error_kind,
                    exec.duration_ms,
                    exec.retention_until,
                    exec.retry_of,
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }

    pub(super) async fn record_execution_finish_impl(
        &self,
        execution_id: String,
        status: ExecutionStatus,
        error_kind: Option<String>,
        finished_at: i64,
    ) -> Result<(), StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            conn.execute(
                "UPDATE executions
                    SET status = ?1,
                        error_kind = ?2,
                        finished_at = ?3,
                        duration_ms = ?3 - started_at
                  WHERE id = ?4",
                params![status.as_str(), error_kind, finished_at, execution_id],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }

    pub(super) async fn record_execution_step_impl(
        &self,
        step: ExecutionStepRow,
    ) -> Result<(), StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            conn.execute(
                "INSERT INTO execution_steps (
                    execution_id, step_index, step_kind, connector, action,
                    started_at, finished_at, status, input_bytes, output_bytes,
                    output_kind, error_kind, input_blob_ref, output_blob_ref
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?5,
                    ?6, ?7, ?8, ?9, ?10,
                    ?11, ?12, ?13, ?14
                 )",
                params![
                    step.execution_id,
                    step.step_index,
                    step.step_kind,
                    step.connector,
                    step.action,
                    step.started_at,
                    step.finished_at,
                    step.status.as_str(),
                    step.input_bytes,
                    step.output_bytes,
                    step.output_kind,
                    step.error_kind,
                    step.input_blob_ref,
                    step.output_blob_ref,
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }

    pub(super) async fn list_executions_impl(
        &self,
        filter: ExecutionFilter,
    ) -> Result<Vec<ExecutionSummary>, StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;

            // Build the WHERE clause incrementally so we only emit
            // the filter clauses the caller asked for. Always
            // ORDER BY started_at DESC for newest-first.
            let mut where_clauses: Vec<&'static str> = Vec::new();
            let mut params_vec: Vec<rusqlite::types::Value> = Vec::new();

            if let Some(ref bot_id) = filter.bot_id {
                where_clauses.push("bot_id = ?");
                params_vec.push(bot_id.clone().into());
            }
            if let Some(ref formation_id) = filter.formation_id {
                where_clauses.push("formation_id = ?");
                params_vec.push(formation_id.clone().into());
            }
            if let Some(ref rule_id) = filter.rule_id {
                where_clauses.push("rule_id = ?");
                params_vec.push(rule_id.clone().into());
            }
            if let Some(status) = filter.status {
                where_clauses.push("status = ?");
                params_vec.push(status.as_str().to_owned().into());
            }
            if let Some(before) = filter.before {
                where_clauses.push("started_at < ?");
                params_vec.push(before.into());
            }

            let limit = filter.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as i64;
            params_vec.push(limit.into());

            let where_sql = if where_clauses.is_empty() {
                String::new()
            } else {
                format!(" WHERE {}", where_clauses.join(" AND "))
            };
            let sql = format!(
                "SELECT id, bot_id, formation_id, rule_id, recipe_id,
                        started_at, finished_at, mode, status, momentum,
                        trigger_summary, duration_ms, error_kind
                   FROM executions
                  {where_sql}
                  ORDER BY started_at DESC
                  LIMIT ?"
            );

            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(rusqlite::params_from_iter(params_vec.iter()), |row| {
                let mode_str: String = row.get(7)?;
                let status_str: String = row.get(8)?;
                let momentum_str: Option<String> = row.get(9)?;
                Ok(ExecutionSummary {
                    id: row.get(0)?,
                    bot_id: row.get(1)?,
                    formation_id: row.get(2)?,
                    rule_id: row.get(3)?,
                    recipe_id: row.get(4)?,
                    started_at: row.get(5)?,
                    finished_at: row.get(6)?,
                    mode: mode_str.parse().unwrap_or(ExecutionMode::Manual),
                    status: status_str.parse().unwrap_or(ExecutionStatus::Failed),
                    momentum: momentum_str.as_deref().and_then(|s| s.parse().ok()),
                    trigger_summary: row.get(10)?,
                    duration_ms: row.get(11)?,
                    error_kind: row.get(12)?,
                })
            })?;

            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }

    pub(super) async fn get_execution_steps_impl(
        &self,
        execution_id: String,
    ) -> Result<Vec<ExecutionStepRow>, StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let mut stmt = conn.prepare(
                "SELECT execution_id, step_index, step_kind, connector, action,
                        started_at, finished_at, status, input_bytes, output_bytes,
                        output_kind, error_kind, input_blob_ref, output_blob_ref
                   FROM execution_steps
                  WHERE execution_id = ?1
                  ORDER BY step_index ASC",
            )?;
            let rows = stmt.query_map(params![execution_id], |row| {
                let status_str: String = row.get(7)?;
                Ok(ExecutionStepRow {
                    execution_id: row.get(0)?,
                    step_index: row.get(1)?,
                    step_kind: row.get(2)?,
                    connector: row.get(3)?,
                    action: row.get(4)?,
                    started_at: row.get(5)?,
                    finished_at: row.get(6)?,
                    status: status_str.parse().unwrap_or(StepStatus::Failed),
                    input_bytes: row.get(8)?,
                    output_bytes: row.get(9)?,
                    output_kind: row.get(10)?,
                    error_kind: row.get(11)?,
                    input_blob_ref: row.get(12)?,
                    output_blob_ref: row.get(13)?,
                })
            })?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }

    pub(super) async fn vacuum_executions_impl(&self, now_ms: i64) -> Result<u64, StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            // Step rows cascade-delete via the FK ON DELETE CASCADE.
            let purged = conn.execute(
                "DELETE FROM executions WHERE retention_until <= ?1",
                params![now_ms],
            )?;
            Ok(purged as u64)
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::SqliteBackend;
    use crate::backend::trait_::StorageBackend;

    fn sample_exec(id: &str, started: i64) -> ExecutionRow {
        ExecutionRow {
            id: id.into(),
            bot_id: Some("bot1".into()),
            formation_id: Some("formA".into()),
            rule_id: Some("rule1".into()),
            recipe_id: Some("scheduled-web-fetch".into()),
            started_at: started,
            finished_at: None,
            mode: ExecutionMode::Cron,
            status: ExecutionStatus::Running,
            momentum: Some(MomentumTag::Warming),
            trigger_summary: Some("Cron 0 7 * * *".into()),
            error_kind: None,
            duration_ms: None,
            retention_until: started + (14 * 24 * 3600 * 1000),
            retry_of: None,
        }
    }

    fn sample_step(execution_id: &str, idx: i64) -> ExecutionStepRow {
        ExecutionStepRow {
            execution_id: execution_id.into(),
            step_index: idx,
            step_kind: "run_connector".into(),
            connector: Some("connector-http".into()),
            action: Some("get".into()),
            started_at: 1000,
            finished_at: Some(1100),
            status: StepStatus::Succeeded,
            input_bytes: 0,
            output_bytes: 1234,
            output_kind: Some("json".into()),
            error_kind: None,
            input_blob_ref: None,
            output_blob_ref: None,
        }
    }

    #[tokio::test]
    async fn record_start_then_finish_round_trips() {
        let store = SqliteBackend::open_in_memory().unwrap();
        store
            .record_execution_start(sample_exec("01HX1", 1000))
            .await
            .unwrap();
        store
            .record_execution_finish("01HX1", ExecutionStatus::Succeeded, None, 2000)
            .await
            .unwrap();

        let list = store
            .list_executions(ExecutionFilter::default())
            .await
            .unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "01HX1");
        assert_eq!(list[0].status, ExecutionStatus::Succeeded);
        assert_eq!(list[0].duration_ms, Some(1000));
    }

    #[tokio::test]
    async fn record_steps_returns_them_ordered() {
        let store = SqliteBackend::open_in_memory().unwrap();
        store
            .record_execution_start(sample_exec("01HX2", 1000))
            .await
            .unwrap();
        // Insert out of order to make sure the ORDER BY does work.
        store
            .record_execution_step(sample_step("01HX2", 2))
            .await
            .unwrap();
        store
            .record_execution_step(sample_step("01HX2", 1))
            .await
            .unwrap();
        let steps = store.get_execution_steps("01HX2").await.unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].step_index, 1);
        assert_eq!(steps[1].step_index, 2);
    }

    #[tokio::test]
    async fn list_filters_by_bot_id() {
        let store = SqliteBackend::open_in_memory().unwrap();
        let mut e1 = sample_exec("01HXA", 1000);
        e1.bot_id = Some("botA".into());
        let mut e2 = sample_exec("01HXB", 2000);
        e2.bot_id = Some("botB".into());

        store.record_execution_start(e1).await.unwrap();
        store.record_execution_start(e2).await.unwrap();

        let list = store
            .list_executions(ExecutionFilter {
                bot_id: Some("botA".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "01HXA");
    }

    #[tokio::test]
    async fn list_orders_newest_first_and_paginates_by_before() {
        let store = SqliteBackend::open_in_memory().unwrap();
        for (i, ts) in [(0, 1000), (1, 2000), (2, 3000)].iter() {
            let id = format!("01HX{i}");
            store
                .record_execution_start(sample_exec(&id, *ts))
                .await
                .unwrap();
        }
        let list = store
            .list_executions(ExecutionFilter::default())
            .await
            .unwrap();
        assert_eq!(list.len(), 3);
        assert!(list[0].started_at >= list[1].started_at);
        assert!(list[1].started_at >= list[2].started_at);

        // Pagination: cursor at started_at < 3000 → only ts=1000 and 2000.
        let page2 = store
            .list_executions(ExecutionFilter {
                before: Some(3000),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(page2.len(), 2);
        assert!(page2.iter().all(|s| s.started_at < 3000));
    }

    #[tokio::test]
    async fn vacuum_purges_expired_rows_and_cascades_to_steps() {
        let store = SqliteBackend::open_in_memory().unwrap();
        let mut e1 = sample_exec("01HXX", 1000);
        e1.retention_until = 5_000; // expires at t=5000
        store.record_execution_start(e1).await.unwrap();
        store
            .record_execution_step(sample_step("01HXX", 1))
            .await
            .unwrap();

        // Vacuum at t=10_000 — should purge.
        let purged = store.vacuum_executions(10_000).await.unwrap();
        assert_eq!(purged, 1);

        let steps = store.get_execution_steps("01HXX").await.unwrap();
        assert!(steps.is_empty(), "FK cascade should remove step rows");
    }

    #[tokio::test]
    async fn vacuum_leaves_unexpired_rows_alone() {
        let store = SqliteBackend::open_in_memory().unwrap();
        store
            .record_execution_start(sample_exec("01HXY", 1000))
            .await
            .unwrap();
        let purged = store.vacuum_executions(0).await.unwrap();
        assert_eq!(purged, 0);
        let list = store
            .list_executions(ExecutionFilter::default())
            .await
            .unwrap();
        assert_eq!(list.len(), 1);
    }
}
