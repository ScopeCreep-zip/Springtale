use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

use springtale_runtime::operations::events::{self, EventListParams};

use super::state::AppState;

/// GET /events — paginated event log.
///
/// Returns recent events (trigger type, connector, timestamp, action taken).
/// Event payloads are NOT stored (ephemeral in PipelineContext per privacy model).
/// The limit clamp lives in `operations::events::list`.
#[utoipa::path(
    get, operation_id = "events_list",
    path = "/events",
    tag = "events",
    params(EventListParams),
    responses((status = 200, description = "Page of the event log", body = Vec<Object>))
)]
pub async fn list(
    State(state): State<AppState>,
    Query(params): Query<EventListParams>,
) -> Result<impl IntoResponse, StatusCode> {
    let page = events::list(&state.runtime, params).await.map_err(|e| {
        tracing::error!(error = %e, "failed to fetch events");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(page))
}
