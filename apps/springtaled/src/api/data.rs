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
