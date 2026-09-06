use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use springtale_runtime::operations;

use super::state::AppState;

/// POST /data/export
pub async fn export_data(State(state): State<AppState>) -> Result<impl IntoResponse, StatusCode> {
    let data = operations::data::export_data(&*state.runtime.store)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(data))
}

/// Body of `POST /data/purge`. Purge destroys every rule, connector,
/// event, session, and memory row in the store, so the confirmation is
/// part of the wire format: a stray POST with no body cannot wipe it.
#[derive(serde::Deserialize)]
pub struct PurgeBody {
    /// Must be `true`. Anything else is a 400.
    pub confirm: bool,
}

/// POST /data/import — restore a snapshot produced by `POST /data/export`.
pub async fn import_data(
    State(state): State<AppState>,
    Json(export): Json<operations::data::DataExport>,
) -> Result<impl IntoResponse, StatusCode> {
    let stats = operations::data::import_data(&*state.runtime.store, export)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(stats))
}

/// POST /data/purge — delete all user data. The vault is left intact.
pub async fn purge_data(
    State(state): State<AppState>,
    Json(body): Json<PurgeBody>,
) -> Result<impl IntoResponse, StatusCode> {
    if !body.confirm {
        return Err(StatusCode::BAD_REQUEST);
    }
    operations::data::purge_data(&*state.runtime.store)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({ "purged": true })))
}
