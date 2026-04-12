use springtale_runtime::operations::templates::{self, Template, WriteReport};

/// List all starter project templates.
#[tauri::command]
pub async fn list_templates() -> Result<Vec<&'static Template>, String> {
    Ok(templates::list().iter().collect())
}

/// Write a template's files to a daemon-chosen directory.
#[tauri::command]
pub async fn write_template(name: String) -> Result<WriteReport, String> {
    templates::write_to(&name).map_err(|e| e.to_string())
}
