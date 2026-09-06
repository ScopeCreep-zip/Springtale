use std::time::{Duration, Instant};

use axum::Json;
use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, Method, Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

use super::login::{self, StreamTicket};
use super::state::AppState;

/// Authentication middleware for the management API.
///
/// Requires `Authorization: Bearer <token>` on all protected routes.
/// The token is one the daemon *issued* — a session from
/// `POST /auth/login` or a long-lived one from `POST /auth/tokens` —
/// never anything derived from the vault passphrase. Both kinds are
/// stored only as `sha256(token)`; the presented bearer is hashed and
/// looked up, and the stored hash is compared with `subtle`
/// (RustCrypto audited) so a hit and a miss cost the same.
///
/// The old branch that compared the bearer against
/// `derive_api_token_hash(passphrase)` is gone: that value is the login
/// verifier and is no longer a bearer.
pub async fn require_auth(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Bearer header only. SSE routes cannot send headers; they use a
    // one-time ticket (`require_stream_ticket`) instead of a `?token=`
    // fallback so the token never lands in a URL.
    let token = login::bearer(request.headers()).ok_or(StatusCode::UNAUTHORIZED)?;

    login::authenticate(&state, token)
        .await
        .ok_or(StatusCode::UNAUTHORIZED)?;

    Ok(next.run(request).await)
}

/// Lifetime of a stream ticket.
const STREAM_TICKET_TTL: Duration = Duration::from_secs(30);

/// POST /stream/ticket — one-time, 30 s ticket for the SSE routes,
/// issued *against the presented session* (plan 6.6 step 4). The ticket
/// carries the bearer's principal, so logging out or revoking the token
/// kills every outstanding ticket with it.
#[utoipa::path(
    post, operation_id = "auth_issue_stream_ticket",
    path = "/stream/ticket",
    tag = "auth",
    responses((status = 200, description = "One-time SSE ticket", body = Object))
)]
pub async fn issue_stream_ticket(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let presented = login::bearer(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    let principal = login::authenticate(&state, presented)
        .await
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let mut bytes = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut bytes);
    let ticket = hex::encode(bytes);
    state.stream_tickets.lock().await.insert(
        ticket.clone(),
        StreamTicket {
            issued_at: Instant::now(),
            principal,
        },
    );
    Ok(Json(
        serde_json::json!({ "ticket": ticket, "ttl_secs": STREAM_TICKET_TTL.as_secs() }),
    ))
}

/// Auth middleware for the SSE routes: requires `?ticket=<hex>` issued by
/// `issue_stream_ticket`, unexpired, never used before, and whose issuing
/// session is still valid. The ticket is removed on first use, so a
/// leaked URL cannot be replayed.
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
    tickets.retain(|_, tk| tk.issued_at.elapsed() < STREAM_TICKET_TTL);
    let redeemed = tickets.remove(t).ok_or(StatusCode::UNAUTHORIZED)?; // single use
    drop(tickets);

    // The session behind the ticket may have been logged out (or the
    // long-lived token revoked) in the 30 s since it was issued.
    if !login::principal_valid(&state, &redeemed.principal).await {
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

/// Origin validation for the MCP endpoint (MCP transports spec).
///
/// The spec is explicit: "Servers MUST validate the `Origin` header on
/// all incoming connections to prevent DNS rebinding attacks." A browser
/// on `https://evil.example` can reach `http://127.0.0.1:9000` and — with
/// a rebound DNS name — send authenticated-looking requests. The Origin
/// header is the one thing the page cannot forge, so it is the check.
///
/// Accepted: no Origin header at all (non-browser clients — curl, an MCP
/// SDK, the stdio bridge — do not send one), or a loopback origin.
/// Everything else is `403`.
///
/// This is *in addition to* [`require_auth`]; it never replaces it, and
/// the `Mcp-Session-Id` header is never authentication ("MCP Servers MUST
/// NOT use sessions for authentication").
pub async fn require_local_origin(
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    match request.headers().get("origin") {
        None => Ok(next.run(request).await),
        Some(value) => {
            let origin = value.to_str().map_err(|_| StatusCode::FORBIDDEN)?;
            if is_loopback_origin(origin) {
                Ok(next.run(request).await)
            } else {
                tracing::warn!(
                    origin = %origin,
                    "MCP request rejected: non-loopback Origin (DNS rebinding guard)"
                );
                Err(StatusCode::FORBIDDEN)
            }
        }
    }
}

/// Whether an `Origin` header value names a loopback host.
///
/// An origin is `scheme://host[:port]` with no path, so anything with a
/// path component, a userinfo section, or a non-http(s) scheme is
/// rejected outright rather than parsed leniently.
pub(crate) fn is_loopback_origin(origin: &str) -> bool {
    let rest = match origin.split_once("://") {
        Some(("http", rest)) | Some(("https", rest)) => rest,
        _ => return false,
    };
    // No path, query, fragment or userinfo in a well-formed origin.
    if rest.contains(['/', '?', '#', '@']) || rest.is_empty() {
        return false;
    }

    let host = if let Some(stripped) = rest.strip_prefix('[') {
        // IPv6 literal: `[::1]:9000`
        match stripped.split_once(']') {
            Some((host, "")) => host,
            Some((host, port)) if port.starts_with(':') => host,
            _ => return false,
        }
    } else {
        match rest.split(':').next() {
            Some(host) => host,
            None => return false,
        }
    };

    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(addr) => addr.is_loopback(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod origin_tests {
    use super::is_loopback_origin;

    #[test]
    fn test_loopback_origins_accepted() {
        for origin in [
            "http://localhost",
            "http://localhost:9000",
            "http://127.0.0.1:9000",
            "https://127.0.0.1",
            "http://127.5.5.5:1",
            "http://[::1]:9000",
            "http://[::1]",
        ] {
            assert!(is_loopback_origin(origin), "{origin} should be loopback");
        }
    }

    #[test]
    fn test_remote_origins_rejected() {
        for origin in [
            "https://evil.example.com",
            "http://192.168.1.10:9000",
            "http://localhost.evil.com",
            "http://127.0.0.1.evil.com",
            "file://",
            "null",
            "http://user@127.0.0.1",
            "http://127.0.0.1/path",
            "http://[::1",
        ] {
            assert!(
                !is_loopback_origin(origin),
                "{origin} should not be loopback"
            );
        }
    }
}
