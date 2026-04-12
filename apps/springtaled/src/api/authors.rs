use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use super::extractors::ValidatedPath;
use super::state::AppState;

/// GET /authors — list all trusted authors.
pub async fn list(State(state): State<AppState>) -> Result<impl IntoResponse, StatusCode> {
    let configs = state
        .runtime
        .store
        .list_config()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let authors: Vec<serde_json::Value> = configs
        .into_iter()
        .filter_map(|(key, value)| {
            let name = key.strip_prefix("trusted-author:")?;
            let data: serde_json::Value = serde_json::from_str(&value).ok()?;
            Some(serde_json::json!({
                "name": name,
                "pubkey": data.get("pubkey").and_then(|v| v.as_str()).unwrap_or(""),
            }))
        })
        .collect();

    Ok(Json(serde_json::json!({ "authors": authors })))
}

/// POST /authors/{name} — add a trusted author.
pub async fn add(
    State(state): State<AppState>,
    ValidatedPath(name): ValidatedPath,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let pubkey = body
        .get("pubkey")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;

    // Validate pubkey is valid hex and 32 bytes (Ed25519 public key)
    let pubkey_bytes = hex::decode(pubkey).map_err(|_| StatusCode::BAD_REQUEST)?;
    if pubkey_bytes.len() != 32 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let key = format!("trusted-author:{name}");
    let value = serde_json::json!({ "pubkey": pubkey }).to_string();

    state
        .runtime
        .store
        .set_config(&key, &value)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tracing::info!(author = %name, "trusted author added");
    Ok(Json(serde_json::json!({ "name": name, "pubkey": pubkey })))
}

/// DELETE /authors/{name} — remove a trusted author.
pub async fn remove(
    State(state): State<AppState>,
    ValidatedPath(name): ValidatedPath,
) -> Result<impl IntoResponse, StatusCode> {
    let key = format!("trusted-author:{name}");
    state
        .runtime
        .store
        .delete_config(&key)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tracing::info!(author = %name, "trusted author removed");
    Ok(Json(serde_json::json!({ "removed": name })))
}
