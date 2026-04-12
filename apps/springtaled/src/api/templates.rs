//! `GET /templates` + `POST /templates/{name}` — expose
//! `operations::templates`.
//!
//! The daemon picks the destination directory (under `$DATA_DIR/projects/`)
//! to prevent path traversal. The caller never supplies a path.

use axum::Json;
use axum::extract::Path as AxumPath;
use axum::http::StatusCode;

use springtale_runtime::operations::templates::{self, Template, TemplateError, WriteReport};

/// GET /templates — every available starter.
pub async fn list() -> Json<Vec<&'static Template>> {
    Json(templates::list().iter().collect())
}

/// POST /templates/{name} — write the named template.
///
/// The daemon picks the destination directory and returns the path in
/// the response. No `dir` field accepted — the caller cannot influence
/// where files land (OWASP ASVS §12.3).
pub async fn write(
    AxumPath(name): AxumPath<String>,
) -> Result<Json<WriteReport>, (StatusCode, String)> {
    match templates::write_to(&name) {
        Ok(report) => Ok(Json(report)),
        Err(TemplateError::Unknown(_)) => {
            Err((StatusCode::NOT_FOUND, format!("unknown template: {name}")))
        }
        Err(TemplateError::DestinationNotEmpty(_)) | Err(TemplateError::WouldOverwrite(_)) => {
            Err((StatusCode::CONFLICT, "destination is not empty".into()))
        }
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}
