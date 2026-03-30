use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

use super::state::AppState;

/// Authentication middleware for the management API.
///
/// Requires `Authorization: Bearer <token>` header on all protected routes.
/// The token is the hex-encoded HMAC-SHA256(passphrase, "springtale-api-token")
/// hash, which the user derives from their vault passphrase. This avoids
/// managing a separate API key.
///
/// Verification uses constant-time comparison to prevent timing attacks.
pub async fn require_auth(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let token_bytes = hex::decode(token).map_err(|_| StatusCode::UNAUTHORIZED)?;

    // Constant-time comparison: the token IS the hash derived from the passphrase.
    // The client computes it the same way the server did at boot time.
    if token_bytes.len() != 32 || !constant_time_eq(&token_bytes, &state.api_token_hash) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(next.run(request).await)
}

/// Constant-time byte comparison to prevent timing attacks.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}
