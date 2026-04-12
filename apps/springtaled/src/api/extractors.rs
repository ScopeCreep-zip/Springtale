//! Custom Axum extractors for API handlers.

use axum::extract::FromRequestParts;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::http::request::Parts;

/// Path parameter with length validation.
///
/// Replaces manual `validate_path_param()` calls in every handler.
/// Rejects path segments longer than `MAX_PATH_SEGMENT_LEN` (256 bytes)
/// to prevent DoS via oversized route strings.
pub struct ValidatedPath(pub String);

impl<S: Send + Sync> FromRequestParts<S> for ValidatedPath {
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Path(param) = Path::<String>::from_request_parts(parts, state)
            .await
            .map_err(|_| StatusCode::BAD_REQUEST)?;
        if param.len() > super::MAX_PATH_SEGMENT_LEN {
            return Err(StatusCode::BAD_REQUEST);
        }
        Ok(ValidatedPath(param))
    }
}
