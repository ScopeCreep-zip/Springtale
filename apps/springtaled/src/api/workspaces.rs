//! HTTP routes for the D1 external-workspace directory and the
//! Track D one-click Onboard stream — the same
//! `operations::workspaces` calls the desktop IPC commands make
//! (plan 2.5). `onboard` is SSE under the stream-ticket layer.

use std::convert::Infallible;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, watch};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

use springtale_cooperation::cadence::AgentId;
use springtale_runtime::operations::workspaces::query::{self, WorkspaceInfo};
use springtale_runtime::operations::workspaces::stream::{
    OnDiscoveryCallback, start_onboard_stream,
};

use super::state::AppState;

/// Default `/start <payload>` marker — same as the desktop command.
const DEFAULT_ONBOARD_PAYLOAD: &str = "springtale-onboard";
/// SSE event name carrying one discovered workspace.
pub const EVENT_NAME_CHAT_DISCOVERED: &str = "chat-discovered";
/// Bounded per-stream buffer; the runtime stops after the first match.
const DISCOVERY_BUFFER: usize = 16;

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub formation_id: String,
    #[serde(default)]
    pub connector: Option<String>,
}

/// GET /workspaces?formation_id=..&connector=.. — directory listing.
pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<WorkspaceInfo>>, (StatusCode, String)> {
    query::list_workspaces(
        &state.runtime.store,
        &q.formation_id,
        q.connector.as_deref(),
    )
    .await
    .map(Json)
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

#[derive(Debug, Deserialize)]
pub struct ScanBody {
    pub formation_id: String,
    pub connector_name: String,
}

/// POST /workspaces/scan — active `discover_destinations` sweep.
pub async fn scan(
    State(state): State<AppState>,
    Json(body): Json<ScanBody>,
) -> Result<Json<Vec<WorkspaceInfo>>, (StatusCode, String)> {
    query::scan_workspaces(&state.runtime, &body.formation_id, &body.connector_name)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))
}

#[derive(Debug, Deserialize)]
pub struct DeleteQuery {
    pub formation_id: String,
    pub workspace_key: String,
}

/// DELETE /workspaces?formation_id=..&workspace_key=..
pub async fn delete(
    State(state): State<AppState>,
    Query(q): Query<DeleteQuery>,
) -> Result<StatusCode, (StatusCode, String)> {
    query::delete_workspace(&state.runtime.store, &q.formation_id, &q.workspace_key)
        .await
        .map(|()| StatusCode::NO_CONTENT)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

#[derive(Debug, Deserialize)]
pub struct UpsertManualBody {
    pub formation_id: String,
    pub workspace_key: String,
    pub display_name: String,
    pub connector_name: String,
    pub kind: String,
}

/// POST /workspaces — manual-entry escape hatch. `entered_by` is a
/// fresh `AgentId` until Phase 3 auth, same as the desktop command.
pub async fn upsert_manual(
    State(state): State<AppState>,
    Json(body): Json<UpsertManualBody>,
) -> Result<StatusCode, (StatusCode, String)> {
    query::upsert_workspace_manual(
        &state.runtime.store,
        &body.formation_id,
        AgentId::default(),
        body.workspace_key,
        body.display_name,
        body.connector_name,
        body.kind,
    )
    .await
    .map(|()| StatusCode::NO_CONTENT)
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// Body for both onboarding routes. `config` is the not-yet-deployed
/// connector config from the deploy form (bot token etc.) — it
/// travels in the body, never the URL.
#[derive(Debug, Deserialize)]
pub struct OnboardBody {
    pub connector_name: String,
    pub config: serde_json::Value,
    #[serde(default)]
    pub payload: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
}

#[derive(Serialize)]
pub struct OnboardUrlResponse {
    pub url: String,
}

/// POST /workspaces/onboard-url — resolve the connector's deep link.
pub async fn onboard_url(
    Json(body): Json<OnboardBody>,
) -> Result<Json<OnboardUrlResponse>, (StatusCode, String)> {
    let payload = body
        .payload
        .unwrap_or_else(|| DEFAULT_ONBOARD_PAYLOAD.to_owned());
    query::preview_onboard_url(&body.connector_name, body.config, &payload)
        .await
        .map(|url| Json(OnboardUrlResponse { url }))
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))
}

/// Cancels the runtime onboarding task when the SSE stream is dropped
/// (client disconnect) — the web analogue of `cancelOnboardStream`.
struct CancelOnDrop(watch::Sender<bool>);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        let _ = self.0.send(true);
    }
}

/// POST /workspaces/onboard?ticket=.. — SSE of `chat-discovered`
/// frames (same payload as the desktop `ChatDiscovered` event) until
/// the first match, the 60 s window, or client disconnect.
pub async fn onboard(
    Json(body): Json<OnboardBody>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)>
{
    let payload = body
        .payload
        .unwrap_or_else(|| DEFAULT_ONBOARD_PAYLOAD.to_owned());
    let session_id = body.session_id.unwrap_or_default();
    let (tx, rx) = mpsc::channel::<serde_json::Value>(DISCOVERY_BUFFER);
    let on_discover: OnDiscoveryCallback = Arc::new(move |info: WorkspaceInfo, matched: bool| {
        let frame = serde_json::json!({
            "session_id": session_id,
            "workspace_key": info.workspace_key,
            "display_name": info.display_name,
            "kind": info.kind,
            "metadata_json": info.metadata_json,
            "matched": matched,
        });
        let _ = tx.try_send(frame);
    });
    let cancel = start_onboard_stream(body.connector_name, body.config, payload, on_discover)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let guard = CancelOnDrop(cancel);
    let stream = ReceiverStream::new(rx).map(move |frame| {
        let _held_until_disconnect = &guard;
        Ok::<_, Infallible>(
            Event::default()
                .event(EVENT_NAME_CHAT_DISCOVERED)
                .data(frame.to_string()),
        )
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
