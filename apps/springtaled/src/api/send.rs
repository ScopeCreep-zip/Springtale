//! `POST /send` — cross-channel messaging endpoint.
//!
//! Thin handler that delegates to
//! `springtale_runtime::operations::cross_channel::send`. Lets any
//! frontend (CLI, Tauri, web dashboard, future AI tool-call) reach the
//! same normalized send path without reimplementing the payload shape.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

use springtale_runtime::operations::cross_channel::{self, SendOutcome, SendRequest};

use super::state::AppState;

/// POST /send — send a message to a channel on a specific connector.
///
/// Request body:
/// ```json
/// { "connector": "connector-telegram", "channel_id": "123", "text": "hi" }
/// ```
#[utoipa::path(
    post, operation_id = "send_send",
    path = "/send",
    tag = "send",
    request_body = SendRequest,
    responses((status = 200, description = "Send outcome", body = SendOutcome))
)]
pub async fn send(
    State(state): State<AppState>,
    Json(req): Json<SendRequest>,
) -> Result<Json<SendOutcome>, (StatusCode, String)> {
    cross_channel::send(&state.runtime, req)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))
}
