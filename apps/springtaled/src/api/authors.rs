//! Trusted-author HTTP surface. All validation lives in
//! `springtale_runtime::operations::authors`.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use springtale_runtime::operations::authors::{self, AddAuthorRequest};

use super::extractors::ValidatedPath;
use super::state::AppState;

/// GET /authors — list all trusted authors.
#[utoipa::path(
    get, operation_id = "authors_list",
    path = "/authors",
    tag = "authors",
    responses((status = 200, description = "Trusted authors", body = Vec<Object>))
)]
pub async fn list(State(state): State<AppState>) -> Result<impl IntoResponse, StatusCode> {
    let authors = authors::list(&*state.runtime.store)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({ "authors": authors })))
}

/// POST /authors/{name} — add a trusted author.
#[utoipa::path(
    post, operation_id = "authors_add",
    path = "/authors/{name}",
    tag = "authors",
    params(("name" = String, Path, description = "Author name")),
    request_body = AddAuthorRequest,
    responses((status = 200, description = "Author added", body = Object))
)]
pub async fn add(
    State(state): State<AppState>,
    ValidatedPath(name): ValidatedPath,
    Json(req): Json<AddAuthorRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let author = authors::add(&*state.runtime.store, &name, &req.pubkey)
        .await
        .map_err(|e| match e {
            springtale_runtime::OperationError::Validation(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        })?;
    Ok(Json(author))
}

/// DELETE /authors/{name} — remove a trusted author.
#[utoipa::path(
    delete, operation_id = "authors_remove",
    path = "/authors/{name}",
    tag = "authors",
    params(("name" = String, Path, description = "Author name")),
    responses((status = 200, description = "Author removed", body = Object))
)]
pub async fn remove(
    State(state): State<AppState>,
    ValidatedPath(name): ValidatedPath,
) -> Result<impl IntoResponse, StatusCode> {
    authors::remove(&*state.runtime.store, &name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({ "removed": name })))
}
