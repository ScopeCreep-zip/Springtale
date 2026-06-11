use springtale_runtime::operations::templates::{self, Template, WriteReport};

/// List all starter project templates.
#[tauri::command]
#[specta::specta]
pub async fn list_templates() -> Result<Vec<Template>, String> {
    Ok(templates::list().to_vec())
}

/// Write a template's files to a daemon-chosen directory.
#[tauri::command]
#[specta::specta]
pub async fn write_template(name: String) -> Result<WriteReport, String> {
    templates::write_to(&name).map_err(|e| e.to_string())
}
