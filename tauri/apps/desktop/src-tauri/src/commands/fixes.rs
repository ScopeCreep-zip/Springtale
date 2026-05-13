use springtale_runtime::operations::error_fixes::{self, FixGuide, FixOutcome};

/// List all known error fix guides.
#[tauri::command]
#[specta::specta]
pub async fn list_fixes() -> Result<Vec<FixGuide>, String> {
    Ok(error_fixes::all_guides().iter().cloned().collect())
}

/// Look up a fix guide by error ID (e.g. "E001").
#[tauri::command]
#[specta::specta]
pub async fn get_fix(id: String) -> Result<FixGuide, String> {
    error_fixes::lookup(&id)
        .cloned()
        .ok_or_else(|| format!("unknown error id: {id}"))
}

/// Attempt an automated fix for the given error ID.
#[tauri::command]
#[specta::specta]
pub async fn apply_fix(id: String) -> Result<FixOutcome, String> {
    Ok(error_fixes::auto_fix(&id).await)
}
