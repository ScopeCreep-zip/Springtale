//! `GET /diagnostics` — expose `operations::diagnostics::run_default_checks`.
//!
//! Thin wrapper. Lets Tauri desktop and web dashboard render the same
//! doctor view the CLI uses without re-implementing check logic.

use axum::Json;
use axum::http::StatusCode;

use springtale_runtime::operations::diagnostics::{self, CallerContext, Report};

/// GET /diagnostics — run every check and return the structured report.
pub async fn list() -> Result<Json<Report>, (StatusCode, String)> {
    let report = diagnostics::run_default_checks(CallerContext::Api).await;
    Ok(Json(report))
}
