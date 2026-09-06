//! HTTP routes for the Phase B executions log — the same
//! `operations::executions` calls the desktop IPC commands make,
//! mirrored so the web dashboard reaches every operation the
//! desktop can (plan 2.5).

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use serde::Serialize;

use springtale_runtime::operations::executions::{
    self, ExecutionFilterIpc, ExecutionInfo, ExecutionStepInfo, GetStepsError, ListExecutionsError,
};

use super::extractors::ValidatedPath;
use super::state::AppState;

/// GET /executions — newest-first, filtered by query params
/// (`bot_id`, `formation_id`, `rule_id`, `status`, `before`, `limit`).
pub async fn list(
    State(state): State<AppState>,
    Query(filter): Query<ExecutionFilterIpc>,
) -> Result<Json<Vec<ExecutionInfo>>, (StatusCode, String)> {
    executions::list_executions_ipc(&state.runtime.store, filter)
        .await
        .map(Json)
        .map_err(|e| {
            let status = match &e {
                ListExecutionsError::Invalid(_) => StatusCode::BAD_REQUEST,
                ListExecutionsError::Operation(_) => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, e.to_string())
        })
}

/// GET /executions/{id}/steps — every step row for one execution.
pub async fn steps(
    State(state): State<AppState>,
    ValidatedPath(id): ValidatedPath,
) -> Result<Json<Vec<ExecutionStepInfo>>, (StatusCode, String)> {
    executions::get_execution_steps_ipc(&state.runtime.store, &id)
        .await
        .map(Json)
        .map_err(|e| {
            let status = match &e {
                GetStepsError::NotFound(_) => StatusCode::NOT_FOUND,
                GetStepsError::Operation(_) => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, e.to_string())
        })
}

#[derive(Serialize)]
pub struct VacuumResponse {
    pub purged: u64,
}

/// POST /executions/vacuum — drop rows past their retention window.
pub async fn vacuum(
    State(state): State<AppState>,
) -> Result<Json<VacuumResponse>, (StatusCode, String)> {
    let now_ms = chrono::Utc::now().timestamp_millis();
    executions::vacuum_executions(&state.runtime.store, now_ms)
        .await
        .map(|purged| Json(VacuumResponse { purged }))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}
