use axum::Json;
use axum::extract::{Query, State};
use axum::response::IntoResponse;

use springtale_store::schema::events::EventFilter;

use super::state::AppState;

/// Query parameters for event listing.
#[derive(serde::Deserialize)]
pub struct EventsQuery {
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(default)]
    pub offset: u32,
    #[serde(default)]
    pub connector: Option<String>,
}

fn default_limit() -> u32 {
    50
}

/// Maximum events per request. Prevents OOM from unbounded queries.
const MAX_EVENT_LIMIT: u32 = 10_000;

/// GET /events — paginated event log.
///
/// Returns recent events (trigger type, connector, timestamp, action taken).
/// Event payloads are NOT stored (ephemeral in PipelineContext per privacy model).
pub async fn list(
    State(state): State<AppState>,
    Query(params): Query<EventsQuery>,
) -> impl IntoResponse {
    let clamped_limit = params.limit.min(MAX_EVENT_LIMIT);

    let filter = EventFilter {
        connector_name: params.connector.clone(),
        limit: Some(clamped_limit),
        offset: if params.offset > 0 {
            Some(params.offset)
        } else {
            None
        },
        ..Default::default()
    };

    let events = springtale_runtime::operations::events::list_events(&state.runtime, &filter).await;

    match events {
        Ok(events) => Json(serde_json::json!({
            "events": events,
            "limit": clamped_limit,
            "offset": params.offset,
        })),
        Err(_) => Json(serde_json::json!({
            "events": [],
            "error": "failed to fetch events",
        })),
    }
}
