//! `GET /fixes`, `GET /fixes/{id}`, `POST /fixes/{id}/apply` — expose
//! `operations::error_fixes`.
//!
//! Every frontend that shows a human-readable remediation for an
//! `OperationError` calls through here. Keeps the guidance table in one
//! place (the runtime ops module) and lets dashboards wire a single
//! "Fix it" button to an auto-fix action without inlining any of the
//! logic.

use axum::Json;
use axum::extract::Path;
use axum::http::StatusCode;

use springtale_runtime::operations::error_fixes::{self, FixGuide, FixOutcome};

/// GET /fixes — static guide table, stable across the session.
pub async fn list() -> Json<Vec<&'static FixGuide>> {
    Json(error_fixes::all_guides().iter().collect())
}

/// GET /fixes/{id} — single guide lookup.
pub async fn get(Path(id): Path<String>) -> Result<Json<&'static FixGuide>, (StatusCode, String)> {
    error_fixes::lookup(&id)
        .map(Json)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("unknown error id: {id}")))
}

/// POST /fixes/{id}/apply — attempt the automated fix for this error.
pub async fn apply(Path(id): Path<String>) -> Json<FixOutcome> {
    Json(error_fixes::auto_fix(&id).await)
}
