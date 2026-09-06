//! HTTP routes for Phase C drift reports — one
//! `operations::executions::drift` call each (plan 2.5).

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;

use springtale_runtime::operations::executions::{self, DriftFilter, DriftReport};

use super::extractors::ValidatedPath;
use super::state::AppState;

/// GET /drift/recipe/{id} — drift window for one recipe.
pub async fn recipe(
    State(state): State<AppState>,
    ValidatedPath(id): ValidatedPath,
    Query(filter): Query<DriftFilter>,
) -> Result<Json<DriftReport>, (StatusCode, String)> {
    executions::recipe_drift(&state.runtime.store, &id, filter)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// GET /drift/rule/{id} — drift window for one rule.
pub async fn rule(
    State(state): State<AppState>,
    ValidatedPath(id): ValidatedPath,
    Query(mut filter): Query<DriftFilter>,
) -> Result<Json<DriftReport>, (StatusCode, String)> {
    filter.rule_id = Some(id);
    executions::rule_drift(&state.runtime.store, filter)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}
