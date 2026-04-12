//! `GET /onboarding/platforms` + `POST /onboarding/{platform}` — expose
//! `operations::onboarding`.
//!
//! The Tauri first-run wizard calls these to render the platform list
//! and persist the user's answers. The CLI's `springtale init` already
//! calls the same operations directly, so the two wizards stay
//! byte-for-byte equivalent.

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;

use springtale_runtime::operations::onboarding::{self, ApplyReport, PlatformForm};

use super::state::AppState;

/// GET /onboarding/platforms — list every platform form the wizard knows.
pub async fn list() -> Json<Vec<&'static PlatformForm>> {
    Json(onboarding::list_platforms().iter().collect())
}

/// Request body for POST /onboarding/{platform}.
#[derive(Debug, Deserialize)]
pub struct ApplyRequest {
    pub answers: BTreeMap<String, String>,
}

/// POST /onboarding/{platform} — persist a wizard answer set as a
/// connector config entry in the encrypted config_store.
pub async fn apply(
    State(state): State<AppState>,
    Path(platform): Path<String>,
    Json(req): Json<ApplyRequest>,
) -> Result<Json<ApplyReport>, (StatusCode, String)> {
    onboarding::apply_platform(&*state.runtime.store, &platform, req.answers)
        .await
        .map(Json)
        .map_err(|e| {
            let status = if e.to_string().contains("already completed") {
                StatusCode::CONFLICT
            } else {
                StatusCode::BAD_REQUEST
            };
            (status, e.to_string())
        })
}
