//! GET `/cooperation/events` — SSE stream of `CooperationEventEnvelope`s.
//!
//! Phase H3 — verbatim mirror of `events_stream.rs`. Subscribes to
//! `RuntimeState::cooperation_tx` (Phase H2 broadcast bus). Web dashboard
//! uses this for the live cooperation event log; desktop uses the Tauri
//! `subscribe_cooperation` Channel<CooperationEventEnvelope> instead
//! (per E10: Tauri Channel<T> beats repeated emit() for high-rate streams).
//!
//! Format: each frame carries the `serde_json::to_string(&envelope)` of a
//! `springtale_cooperation::CooperationEventEnvelope` — the envelope's
//! Serialize derive provides `kind`-tagged variants per H1.

use std::convert::Infallible;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use tokio_stream::StreamExt;

use super::state::AppState;

/// GET /cooperation/events — Server-Sent Events for cooperation lifecycle.
///
/// Streams every internal-state cooperation event (intervention fired,
/// sacrifice yielded, vote opened, role transformed, member marked down,
/// supervisor escalation, pacing phase change, cascade hit, recovery
/// action, surface deposit, interference event, CFP/replan/commit
/// outcome) as it occurs. Auth required (Bearer token). Read-only.
///
/// Lagged subscribers silently drop missed events — matches the
/// `/events/stream` precedent.
pub async fn stream(
    State(state): State<AppState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.runtime.cooperation_tx.subscribe();

    let stream = tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(|result| {
        match result {
            Ok(envelope) => {
                let data = serde_json::to_string(&envelope).unwrap_or_default();
                Some(Ok(Event::default().data(data)))
            }
            Err(_) => None, // Lagged — skip missed envelopes
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}
