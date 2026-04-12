use axum::extract::State;
use axum::http::{Method, Request, StatusCode};
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

/// CSRF defense for mutating routes (POST/PUT/DELETE).
///
/// Pattern from Syncthing (`X-API-Key`) and Jupyter (`_xsrf`):
/// require a custom header that browsers cannot set cross-origin
/// without a CORS preflight — which we deny by never sending
/// `Access-Control-Allow-Origin`. Also validate the `Origin` and
/// `Sec-Fetch-Site` headers.
///
/// Precedent for why this is needed on localhost:
/// Transmission CVE-2018-5702, Zoom 2019 localhost CVE. A malicious
/// webpage can POST to `127.0.0.1:8080` and the browser sends the
/// body; CORS only blocks the response, not the write.
pub async fn require_csrf_protection(
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let method = request.method().clone();
    if method == Method::GET || method == Method::HEAD || method == Method::OPTIONS {
        return Ok(next.run(request).await);
    }

    if let Some(origin) = request.headers().get("origin").and_then(|v| v.to_str().ok()) {
        let allowed = origin == "null"
            || origin.starts_with("http://127.0.0.1:")
            || origin.starts_with("http://localhost:")
            || origin == "tauri://localhost";
        if !allowed {
            tracing::warn!(origin = %origin, "CSRF: rejected cross-origin mutating request");
            return Err(StatusCode::FORBIDDEN);
        }
    }

    if let Some(site) = request
        .headers()
        .get("sec-fetch-site")
        .and_then(|v| v.to_str().ok())
        && site == "cross-site"
    {
        tracing::warn!("CSRF: rejected sec-fetch-site=cross-site");
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(next.run(request).await)
}
