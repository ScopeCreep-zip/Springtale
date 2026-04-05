use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use subtle::ConstantTimeEq;

use super::state::AppState;

/// Authentication middleware for the management API.
///
/// Requires `Authorization: Bearer <token>` header on all protected routes.
/// The token is the hex-encoded HMAC-SHA256(passphrase, "springtale-api-token")
/// hash, which the user derives from their vault passphrase. This avoids
/// managing a separate API key.
///
/// Verification uses `subtle::ConstantTimeEq` (RustCrypto audited) to
/// prevent timing attacks.
pub async fn require_auth(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Try Authorization header first (standard path)
    // Fall back to ?token= query parameter for SSE (EventSource limitation —
    // the browser EventSource API cannot send custom headers).
    // This is safe because the dashboard binds 127.0.0.1 only.
    let token = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .or_else(|| {
            request
                .uri()
                .query()
                .and_then(|q| q.split('&').find_map(|pair| pair.strip_prefix("token=")))
        })
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let token_bytes = hex::decode(token).map_err(|_| StatusCode::UNAUTHORIZED)?;

    // Constant-time comparison via `subtle` crate (RustCrypto audited).
    // The token IS the hash derived from the passphrase — client computes
    // it the same way the server did at boot time.
    if token_bytes.len() != 32 || bool::from(!token_bytes.ct_eq(&state.api_token_hash)) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(next.run(request).await)
}
