use tauri::State;

use crate::state::AppState;
use springtale_runtime::operations::diagnostics::{self, CallerContext, Report};

/// Run diagnostic checks (config, vault, database, data dir, connectors).
#[tauri::command]
#[specta::specta]
pub async fn run_diagnostics(state: State<'_, AppState>) -> Result<Report, String> {
    // Diagnostics don't require the runtime to be initialized — they
    // inspect the filesystem and config independently. Use Api context
    // because the desktop app IS the running process (port check would
    // false-alarm since we own it).
    let _ = &state; // suppress unused warning; diagnostics are runtime-independent
    Ok(diagnostics::run_default_checks(CallerContext::Api).await)
}
