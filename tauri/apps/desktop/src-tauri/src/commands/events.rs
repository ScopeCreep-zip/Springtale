use tauri::State;

use springtale_runtime::operations::events::EventInfo;
use springtale_store::schema::events::EventFilter;

use crate::runtime_guard::require_runtime;
use crate::state::AppState;

/// List recent events.
#[tauri::command]
#[specta::specta]
pub async fn list_events(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> Result<Vec<EventInfo>, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();

    let filter = EventFilter {
        limit: Some(limit.unwrap_or(50)),
        ..EventFilter::default()
    };

    let events = springtale_runtime::operations::events::list_events(rt, &filter)
        .await
        .map_err(|e| format!("failed to list events: {e}"))?;

    Ok(events.into_iter().map(EventInfo::from).collect())
}
