//! Read-side operations for the executions log.
//!
//! Tauri command handlers + the web dashboard call these. They
//! return either the store types directly (`*Row` shapes —
//! privacy-shaped already) or the IPC projections in this module
//! ([`ExecutionInfo`] / [`ExecutionStepInfo`]) that derive
//! `specta::Type` for desktop IPC.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use specta::Type;
use springtale_store::backend::StorageBackend;
use springtale_store::schema::executions::{
    ExecutionFilter, ExecutionStepRow, ExecutionSummary,
};
use thiserror::Error;

use crate::error::OperationError;

/// IPC-shaped projection of [`ExecutionSummary`]. Flat strings —
/// no recursion — per `feedback_specta_recursive_types`. Tauri
/// commands return `Vec<ExecutionInfo>`; the web dashboard reads
/// the JSON form of the same shape.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ExecutionInfo {
    pub id: String,
    pub bot_id: Option<String>,
    pub formation_id: Option<String>,
    pub rule_id: Option<String>,
    pub recipe_id: Option<String>,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub duration_ms: Option<i64>,
    /// `cron | webhook | connector_event | file_watch | manual | cooperation | retry | dry_run`.
    pub mode: String,
    /// `running | succeeded | empty | failed | aborted | timed_out`.
    pub status: String,
    /// `cold | warming | hot | fever` — None when momentum unknown.
    pub momentum: Option<String>,
    pub trigger_summary: Option<String>,
    pub error_kind: Option<String>,
}

impl From<ExecutionSummary> for ExecutionInfo {
    fn from(s: ExecutionSummary) -> Self {
        Self {
            id: s.id,
            bot_id: s.bot_id,
            formation_id: s.formation_id,
            rule_id: s.rule_id,
            recipe_id: s.recipe_id,
            started_at: s.started_at,
            finished_at: s.finished_at,
            duration_ms: s.duration_ms,
            mode: s.mode.as_str().to_owned(),
            status: s.status.as_str().to_owned(),
            momentum: s.momentum.map(|m| m.as_str().to_owned()),
            trigger_summary: s.trigger_summary,
            error_kind: s.error_kind,
        }
    }
}

/// IPC-shaped projection of [`ExecutionStepRow`]. Flat. Sizes-only —
/// content blob refs forwarded as opaque strings for Phase C.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ExecutionStepInfo {
    pub execution_id: String,
    pub step_index: i64,
    pub step_kind: String,
    pub connector: Option<String>,
    pub action: Option<String>,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    /// `succeeded | failed | suppressed | skipped`.
    pub status: String,
    pub input_bytes: i64,
    pub output_bytes: i64,
    pub output_kind: Option<String>,
    pub error_kind: Option<String>,
    pub input_blob_ref: Option<String>,
    pub output_blob_ref: Option<String>,
}

impl From<ExecutionStepRow> for ExecutionStepInfo {
    fn from(r: ExecutionStepRow) -> Self {
        Self {
            execution_id: r.execution_id,
            step_index: r.step_index,
            step_kind: r.step_kind,
            connector: r.connector,
            action: r.action,
            started_at: r.started_at,
            finished_at: r.finished_at,
            status: r.status.as_str().to_owned(),
            input_bytes: r.input_bytes,
            output_bytes: r.output_bytes,
            output_kind: r.output_kind,
            error_kind: r.error_kind,
            input_blob_ref: r.input_blob_ref,
            output_blob_ref: r.output_blob_ref,
        }
    }
}

/// Mirror of [`ExecutionFilter`] for IPC. Flat, derives `Type`
/// (the store filter doesn't because it's not crossing the boundary).
#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
pub struct ExecutionFilterIpc {
    pub bot_id: Option<String>,
    pub formation_id: Option<String>,
    pub rule_id: Option<String>,
    /// One of the status enum strings — `succeeded | failed | ...`.
    pub status: Option<String>,
    pub before: Option<i64>,
    pub limit: Option<u32>,
}

impl ExecutionFilterIpc {
    /// Resolve the IPC filter into the store-side typed filter.
    /// Returns an error string when `status` doesn't match a known
    /// enum tag — keeping the parse on this side so the
    /// `list_executions_ipc` boundary is the single point where the
    /// untrusted IPC payload becomes a typed value.
    pub fn into_typed(self) -> Result<ExecutionFilter, String> {
        let status = match self.status {
            None => None,
            Some(ref s) => Some(
                springtale_store::schema::executions::ExecutionStatus::from_str(s)
                    .ok_or_else(|| format!("unknown status: {s}"))?,
            ),
        };
        Ok(ExecutionFilter {
            bot_id: self.bot_id,
            formation_id: self.formation_id,
            rule_id: self.rule_id,
            status,
            before: self.before,
            limit: self.limit,
        })
    }
}

/// Failure modes for [`list_executions`]. Wraps store errors plus a
/// validation surface for callers that pass bad filter values
/// (e.g. negative `limit`).
#[derive(Debug, Error)]
pub enum ListExecutionsError {
    #[error("invalid filter: {0}")]
    Invalid(String),
    #[error(transparent)]
    Operation(#[from] OperationError),
}

impl From<springtale_store::StoreError> for ListExecutionsError {
    fn from(e: springtale_store::StoreError) -> Self {
        Self::Operation(OperationError::Store(e))
    }
}

#[derive(Debug, Error)]
pub enum GetStepsError {
    #[error("execution not found: {0}")]
    NotFound(String),
    #[error(transparent)]
    Operation(#[from] OperationError),
}

impl From<springtale_store::StoreError> for GetStepsError {
    fn from(e: springtale_store::StoreError) -> Self {
        Self::Operation(OperationError::Store(e))
    }
}

/// List recent executions matching `filter`. Newest-first; caller
/// paginates with the `before` cursor on `started_at`.
pub async fn list_executions(
    store: &Arc<dyn StorageBackend>,
    filter: ExecutionFilter,
) -> Result<Vec<ExecutionSummary>, ListExecutionsError> {
    if let Some(limit) = filter.limit {
        if limit == 0 {
            return Err(ListExecutionsError::Invalid(
                "limit must be > 0; pass None for the default".into(),
            ));
        }
    }
    let rows = store.list_executions(filter).await?;
    Ok(rows)
}

/// Fetch every step row for one execution, ordered by index.
pub async fn get_execution_steps(
    store: &Arc<dyn StorageBackend>,
    execution_id: &str,
) -> Result<Vec<ExecutionStepRow>, GetStepsError> {
    let rows = store.get_execution_steps(execution_id).await?;
    if rows.is_empty() {
        // Returning an empty Vec is fine; the panel renders the
        // "no steps recorded" state. We don't 404 here because a
        // chain may have failed before any step was recorded.
    }
    Ok(rows)
}

/// IPC entry point — accepts the flat IPC filter, returns
/// IPC-shaped summaries. The Tauri command + web dashboard both
/// call this; CLI / internal callers go straight to
/// [`list_executions`].
pub async fn list_executions_ipc(
    store: &Arc<dyn StorageBackend>,
    filter: ExecutionFilterIpc,
) -> Result<Vec<ExecutionInfo>, ListExecutionsError> {
    let typed = filter
        .into_typed()
        .map_err(ListExecutionsError::Invalid)?;
    let rows = list_executions(store, typed).await?;
    Ok(rows.into_iter().map(ExecutionInfo::from).collect())
}

/// IPC entry point for fetching one execution's step rows.
pub async fn get_execution_steps_ipc(
    store: &Arc<dyn StorageBackend>,
    execution_id: &str,
) -> Result<Vec<ExecutionStepInfo>, GetStepsError> {
    let rows = get_execution_steps(store, execution_id).await?;
    Ok(rows.into_iter().map(ExecutionStepInfo::from).collect())
}

/// Drop every executions row whose `retention_until` has elapsed.
/// Called by the background vacuum task — see
/// [`crate::operations::executions::recorder::DEFAULT_RETENTION_MS`]
/// for the default window.
pub async fn vacuum_executions(
    store: &Arc<dyn StorageBackend>,
    now_ms: i64,
) -> Result<u64, OperationError> {
    let purged = store.vacuum_executions(now_ms).await?;
    if purged > 0 {
        tracing::info!(purged, "vacuum_executions removed expired rows");
    }
    Ok(purged)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use springtale_store::SqliteBackend;
    use std::sync::Arc;

    fn store() -> Arc<dyn StorageBackend> {
        Arc::new(SqliteBackend::open_in_memory().unwrap())
    }

    #[tokio::test]
    async fn list_with_zero_limit_is_invalid() {
        let s = store();
        let err = list_executions(
            &s,
            ExecutionFilter {
                limit: Some(0),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ListExecutionsError::Invalid(_)));
    }

    #[tokio::test]
    async fn list_empty_store_returns_empty_vec() {
        let s = store();
        let rows = list_executions(&s, ExecutionFilter::default()).await.unwrap();
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn steps_for_missing_execution_returns_empty_vec() {
        let s = store();
        let rows = get_execution_steps(&s, "01HXMISSING").await.unwrap();
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn vacuum_on_empty_store_returns_zero() {
        let s = store();
        let purged = vacuum_executions(&s, 0).await.unwrap();
        assert_eq!(purged, 0);
    }
}
