use std::convert::Infallible;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use tokio_stream::StreamExt;

use super::state::AppState;

/// GET /canvas/stream — Server-Sent Events for live Canvas/A2UI updates.
///
/// Streams canvas updates as they occur. Auth required (Bearer token).
/// The dashboard uses this for live canvas rendering.
///
/// Same pattern as events_stream.rs — broadcast channel → SSE.
pub async fn stream(
    State(state): State<AppState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.runtime.canvas_tx.subscribe();

    let stream = tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(|result| {
        match result {
            Ok(update) => {
                let data = serde_json::to_string(&update).unwrap_or_default();
                Some(Ok(Event::default().data(data)))
            }
            Err(_) => None, // Lagged subscriber — skip missed updates
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}
