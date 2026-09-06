//! Event operations — query event log.
//!
//! Runtime operations take `&RuntimeState`.
//! Store operations take `&dyn StorageBackend` (CLI uses these).

use serde::{Deserialize, Serialize};

use specta::Type;
use springtale_store::StorageBackend;
use springtale_store::schema::events::{EventEntry, EventFilter};

use crate::error::OperationError;
use crate::state::RuntimeState;

/// Event info — shared presentation type for API and IPC responses.
#[derive(Debug, Serialize, Type)]
pub struct EventInfo {
    pub id: String,
    pub connector_name: String,
    pub trigger_type: String,
    pub timestamp: String,
    pub action_taken: String,
    /// Severity inferred from action_taken text: "ok" | "error".
    pub severity: String,
}

impl From<EventEntry> for EventInfo {
    fn from(e: EventEntry) -> Self {
        let action_lower = e.action_taken.to_lowercase();
        let severity = if action_lower.contains("error")
            || action_lower.contains("fail")
            || action_lower.contains("block")
        {
            "error"
        } else {
            "ok"
        }
        .to_owned();

        Self {
            id: e.id.to_string(),
            connector_name: e.connector_name,
            trigger_type: e.trigger_type,
            timestamp: e.timestamp.to_rfc3339(),
            action_taken: e.action_taken,
            severity,
        }
    }
}

/// List events matching the given filter (runtime operation).
pub async fn list_events(
    state: &RuntimeState,
    filter: &EventFilter,
) -> Result<Vec<EventEntry>, OperationError> {
    state
        .store
        .list_events(filter)
        .await
        .map_err(OperationError::Store)
}

/// List events from the persistent store (no runtime needed).
///
/// Used by CLI which doesn't load the full runtime.
pub async fn list_events_from_store(
    store: &dyn StorageBackend,
    filter: &EventFilter,
) -> Result<Vec<EventEntry>, OperationError> {
    store
        .list_events(filter)
        .await
        .map_err(OperationError::Store)
}

/// Default page size when the caller does not ask for one.
pub const DEFAULT_EVENT_LIMIT: u32 = 50;

/// Maximum events per request. Prevents OOM from unbounded queries.
pub const MAX_EVENT_LIMIT: u32 = 10_000;

fn default_event_limit() -> u32 {
    DEFAULT_EVENT_LIMIT
}

/// Query parameters for a page of the event log.
#[derive(Debug, Clone, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub struct EventListParams {
    #[serde(default = "default_event_limit")]
    pub limit: u32,
    #[serde(default)]
    pub offset: u32,
    #[serde(default)]
    pub connector: Option<String>,
}

impl Default for EventListParams {
    fn default() -> Self {
        Self {
            limit: DEFAULT_EVENT_LIMIT,
            offset: 0,
            connector: None,
        }
    }
}

/// One page of the event log, with the limit actually applied.
#[derive(Debug, Serialize)]
pub struct EventPage {
    pub events: Vec<EventEntry>,
    /// The limit after clamping to [`MAX_EVENT_LIMIT`].
    pub limit: u32,
    pub offset: u32,
}

/// List a page of events, clamping the caller's limit.
///
/// The clamp lives here rather than in a handler so every surface gets
/// the same ceiling.
pub async fn list(
    state: &RuntimeState,
    params: EventListParams,
) -> Result<EventPage, OperationError> {
    let limit = params.limit.min(MAX_EVENT_LIMIT);
    let filter = EventFilter {
        connector_name: params.connector,
        limit: Some(limit),
        offset: (params.offset > 0).then_some(params.offset),
        ..Default::default()
    };
    let events = list_events(state, &filter).await?;
    Ok(EventPage {
        events,
        limit,
        offset: params.offset,
    })
}
