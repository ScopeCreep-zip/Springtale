use std::convert::Infallible;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use tokio_stream::StreamExt;

use super::state::AppState;

/// GET /events/stream — Server-Sent Events for real-time event log.
///
/// Streams new events as they occur. Auth required (Bearer token).
/// Read-only — no write capability.
///
/// The dashboard uses this for live event log updates instead of polling.
pub async fn stream(
    State(state): State<AppState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.event_tx.subscribe();

    let stream = tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(|result| {
        match result {
            Ok(entry) => {
                let json = serde_json::json!({
                    "id": entry.id.to_string(),
                    "connector_name": entry.connector_name,
                    "trigger_type": entry.trigger_type,
                    "timestamp": entry.timestamp.to_rfc3339(),
                    "action_taken": entry.action_taken,
                });
                let data = serde_json::to_string(&json).unwrap_or_default();
                Some(Ok(Event::default().data(data)))
            }
            Err(_) => None, // Lagged subscriber — skip missed events
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}
