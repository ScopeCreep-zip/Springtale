use std::time::{Duration, Instant};

use axum::Json;
use axum::body::Body;
use axum::extract::State;
use axum::http::{Method, Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use rand::RngCore;
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
    // Bearer header only. SSE routes cannot send headers; they use a
    // one-time ticket (`require_stream_ticket`) instead of a `?token=`
    // fallback so the API token never lands in a URL.
    let token = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
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

/// Lifetime of a stream ticket.
const STREAM_TICKET_TTL: Duration = Duration::from_secs(30);

/// POST /stream/ticket — one-time, 30 s ticket for the SSE routes.
pub async fn issue_stream_ticket(State(state): State<AppState>) -> Json<serde_json::Value> {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let ticket = hex::encode(bytes);
    state
        .stream_tickets
        .lock()
        .await
        .insert(ticket.clone(), Instant::now());
    Json(serde_json::json!({ "ticket": ticket, "ttl_secs": STREAM_TICKET_TTL.as_secs() }))
}

/// Auth middleware for the SSE routes: requires `?ticket=<hex>` issued by
/// `issue_stream_ticket`, unexpired and never used before. The ticket is
/// removed on first use, so a leaked URL cannot be replayed.
pub async fn require_stream_ticket(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let t = request
        .uri()
        .query()
        .and_then(|q| q.split('&').find_map(|p| p.strip_prefix("ticket=")))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let mut tickets = state.stream_tickets.lock().await;
    tickets.retain(|_, at| at.elapsed() < STREAM_TICKET_TTL);
    tickets.remove(t).ok_or(StatusCode::UNAUTHORIZED)?; // single use
    drop(tickets);
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

    if let Some(origin) = request
        .headers()
        .get("origin")
        .and_then(|v| v.to_str().ok())
    {
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
