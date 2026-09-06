//! GET `/stream` — one multiplexed SSE stream for the dashboard.
//!
//! Merges the three broadcast buses that used to have their own routes
//! (`/events/stream`, `/canvas/stream`, `/cooperation/events`) into a
//! single connection whose frames carry an SSE `event:` name:
//!
//! - `event`       — `springtale_store::schema::events::EventEntry` summary
//! - `canvas`      — `CanvasUpdate` (A2UI)
//! - `cooperation` — `springtale_cooperation::CooperationEventEnvelope`
//!
//! Payloads are byte-identical to the former per-route streams. One
//! connection per tab keeps the browser's per-origin SSE connection cap
//! (6 when not over HTTP/2) out of reach. Auth is a one-time ticket
//! (`auth::require_stream_ticket`), never a bearer token in the URL.
//! Lagged subscribers silently drop missed frames.

use std::convert::Infallible;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use super::state::AppState;

/// SSE `event:` name for event-log entries.
pub const EVENT_NAME_EVENT: &str = "event";
/// SSE `event:` name for canvas updates.
pub const EVENT_NAME_CANVAS: &str = "canvas";
/// SSE `event:` name for cooperation envelopes.
pub const EVENT_NAME_COOPERATION: &str = "cooperation";

/// The event-log summary shape the dashboard expects (unchanged from the
/// former `/events/stream`).
fn event_entry_json(entry: &springtale_store::schema::events::EventEntry) -> serde_json::Value {
    serde_json::json!({
        "id": entry.id.to_string(),
        "connector_name": entry.connector_name,
        "trigger_type": entry.trigger_type,
        "timestamp": entry.timestamp.to_rfc3339(),
        "action_taken": entry.action_taken,
    })
}

fn frame(name: &'static str, data: String) -> Option<Result<Event, Infallible>> {
    Some(Ok(Event::default().event(name).data(data)))
}

/// GET /stream — Server-Sent Events, multiplexed. Ticket auth. Read-only.
#[utoipa::path(
    get, operation_id = "stream_stream",
    path = "/stream",
    tag = "stream",
    responses((status = 200, description = "text/event-stream of events, canvas and cooperation frames", body = String))
)]
pub async fn stream(
    State(state): State<AppState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let events = BroadcastStream::new(state.event_tx.subscribe()).filter_map(|r| match r {
        Ok(entry) => frame(
            EVENT_NAME_EVENT,
            serde_json::to_string(&event_entry_json(&entry)).unwrap_or_default(),
        ),
        Err(_) => None,
    });
    let canvas =
        BroadcastStream::new(state.runtime.canvas_tx.subscribe()).filter_map(|r| match r {
            Ok(update) => frame(
                EVENT_NAME_CANVAS,
                serde_json::to_string(&update).unwrap_or_default(),
            ),
            Err(_) => None,
        });
    let cooperation =
        BroadcastStream::new(state.runtime.cooperation_tx.subscribe()).filter_map(|r| match r {
            Ok(envelope) => frame(
                EVENT_NAME_COOPERATION,
                serde_json::to_string(&envelope).unwrap_or_default(),
            ),
            Err(_) => None,
        });

    Sse::new(events.merge(canvas).merge(cooperation)).keep_alive(KeepAlive::default())
}
