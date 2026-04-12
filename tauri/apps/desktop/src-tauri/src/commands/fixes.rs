use springtale_runtime::operations::error_fixes::{self, FixGuide, FixOutcome};

/// List all known error fix guides.
#[tauri::command]
pub async fn list_fixes() -> Result<Vec<&'static FixGuide>, String> {
    Ok(error_fixes::all_guides().iter().collect())
}

/// Look up a fix guide by error ID (e.g. "E001").
#[tauri::command]
pub async fn get_fix(id: String) -> Result<&'static FixGuide, String> {
    error_fixes::lookup(&id).ok_or_else(|| format!("unknown error id: {id}"))
}

/// Attempt an automated fix for the given error ID.
#[tauri::command]
pub async fn apply_fix(id: String) -> Result<FixOutcome, String> {
    Ok(error_fixes::auto_fix(&id).await)
}
