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
#[utoipa::path(
    get, operation_id = "executions_list",
    path = "/executions",
    tag = "executions",
    params(ExecutionFilterIpc),
    responses((status = 200, description = "Newest-first executions", body = Vec<ExecutionInfo>))
)]
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
#[utoipa::path(
    get, operation_id = "executions_steps",
    path = "/executions/{id}/steps",
    tag = "executions",
    params(("id" = String, Path, description = "Execution id")),
    responses((status = 200, description = "Steps of one execution", body = Vec<ExecutionStepInfo>))
)]
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

#[derive(Serialize, utoipa::ToSchema)]
pub struct VacuumResponse {
    pub purged: u64,
}

/// POST /executions/vacuum — drop rows past their retention window.
#[utoipa::path(
    post, operation_id = "executions_vacuum",
    path = "/executions/vacuum",
    tag = "executions",
    responses((status = 200, description = "Rows purged", body = VacuumResponse))
)]
pub async fn vacuum(
    State(state): State<AppState>,
) -> Result<Json<VacuumResponse>, (StatusCode, String)> {
    let now_ms = chrono::Utc::now().timestamp_millis();
    executions::vacuum_executions(&state.runtime.store, now_ms)
        .await
        .map(|purged| Json(VacuumResponse { purged }))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}
