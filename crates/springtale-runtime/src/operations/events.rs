//! Event operations — query event log.
//!
//! Runtime operations take `&RuntimeState`.
//! Store operations take `&dyn StorageBackend` (CLI uses these).

use serde::Serialize;

use springtale_store::StorageBackend;
use springtale_store::schema::events::{EventEntry, EventFilter};

use crate::error::OperationError;
use crate::state::RuntimeState;

/// Event info — shared presentation type for API and IPC responses.
#[derive(Debug, Serialize)]
pub struct EventInfo {
    pub id: String,
    pub connector_name: String,
    pub trigger_type: String,
    pub timestamp: String,
    pub action_taken: String,
}

impl From<EventEntry> for EventInfo {
    fn from(e: EventEntry) -> Self {
        Self {
            id: e.id.to_string(),
            connector_name: e.connector_name,
            trigger_type: e.trigger_type,
            timestamp: e.timestamp.to_rfc3339(),
            action_taken: e.action_taken,
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
