//! Login and token issuance (plan 6.6, finding 109).
//!
//! The bearer used to be `HMAC(vault passphrase)` — deterministic, not
//! rotatable, a passphrase equivalent, and something the user was told to
//! compute by hand. It is now *issued*, never derived:
//!
//! - `POST /auth/login` verifies the passphrase (constant-time, against
//!   the same `derive_api_token_hash` value the daemon computed at boot)
//!   and mints a **session** token: 32 bytes (256 bits) straight from the
//!   OS CSPRNG, well past OWASP's "at least 64 bits of entropy", with no
//!   structure at all — OWASP's "the session ID content must be
//!   meaningless". A new value is minted on every login, which is
//!   OWASP's mandatory regeneration at authentication.
//! - `POST /auth/tokens` mints a **long-lived named** token the same way
//!   (the Home Assistant long-lived-access-token shape).
//!
//! Neither kind is ever stored in the clear: sessions live in memory as
//! `sha256(token)` and long-lived tokens as `sha256(token)` in
//! `api_tokens`. The token string is returned exactly once. The
//! passphrase-derived hash stays as the *verifier* and is never accepted
//! as a bearer again.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use rand::RngCore;
use rand::rngs::OsRng;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tokio::sync::Mutex;
use zeroize::Zeroize;

use springtale_crypto::token::derive_api_token_hash;
use springtale_store::schema::api_tokens::ApiTokenRow;

use super::state::AppState;

/// Bytes of entropy in a minted token. 256 bits — OWASP asks for 64.
pub const TOKEN_BYTES: usize = 32;

/// Maximum length of a user-supplied token name.
const MAX_TOKEN_NAME_LEN: usize = 128;

/// How stale `last_used` may get before a long-lived token's row is
/// written again. Keeps a request from costing a DB write.
const TOUCH_INTERVAL_MS: i64 = 60_000;

/// `sha256(token)` — the only form a token is ever stored in.
pub type TokenHash = [u8; 32];

/// One live login session. Held in memory only: locking the vault
/// drops the process state and every session with it.
#[derive(Debug, Clone, Copy)]
pub struct SessionRecord {
    /// When the session was minted (absolute-timeout anchor).
    pub issued_at: Instant,
    /// Last accepted request on it (idle-timeout anchor).
    pub last_seen: Instant,
}

/// Live sessions, keyed by `sha256(token)`.
pub type SessionMap = Arc<Mutex<HashMap<TokenHash, SessionRecord>>>;

/// A one-time SSE ticket, bound to the session that asked for it.
#[derive(Debug, Clone, Copy)]
pub struct StreamTicket {
    /// Issue time — the 30 s TTL runs from here.
    pub issued_at: Instant,
    /// The bearer this ticket was issued against. Revoking that bearer
    /// invalidates the ticket before it can be redeemed.
    pub principal: TokenHash,
}

/// Hash a raw token. The single place `sha256(token)` is computed.
#[must_use]
pub fn hash_token(token: &[u8]) -> TokenHash {
    Sha256::digest(token).into()
}

/// Mint a token: 32 bytes from the operating system's CSPRNG.
///
/// Returns `(hex string, sha256 hash)`. The caller stores the hash and
/// hands the string to the user once; nothing keeps the string.
fn mint_token() -> (String, TokenHash) {
    let mut bytes = [0u8; TOKEN_BYTES];
    OsRng.fill_bytes(&mut bytes);
    let hex = hex::encode(bytes);
    let hash = hash_token(&bytes);
    bytes.zeroize();
    (hex, hash)
}

/// Session idle and absolute timeouts, read from `bot:settings` (6.3).
fn timeouts(state: &AppState) -> (Duration, Duration) {
    let settings = state.runtime.bot_settings.load();
    (
        Duration::from_secs(settings.session_idle_secs),
        Duration::from_secs(settings.session_absolute_secs),
    )
}

/// Whether a session is still alive under both timeouts.
fn session_alive(rec: &SessionRecord, idle: Duration, absolute: Duration, now: Instant) -> bool {
    now.duration_since(rec.last_seen) < idle && now.duration_since(rec.issued_at) < absolute
}

/// Look a hash up in the session map, dropping every expired session on
/// the way through and refreshing `last_seen` on a hit.
///
/// The map is scanned with `subtle::ConstantTimeEq` rather than probed
/// by key so a hit and a miss cost the same comparison work.
async fn session_lookup(state: &AppState, hash: &TokenHash, refresh: bool) -> bool {
    let (idle, absolute) = timeouts(state);
    let now = Instant::now();
    let mut sessions = state.sessions.lock().await;
    sessions.retain(|_, rec| session_alive(rec, idle, absolute, now));
    let mut found: Option<TokenHash> = None;
    // Scan every key with a constant-time compare, without breaking early,
    // so a bearer's validity is not readable from how long the lookup took.
    for key in sessions.keys() {
        if bool::from(key.ct_eq(hash)) {
            found = Some(*key);
        }
    }
    match found {
        Some(key) => {
            if refresh && let Some(rec) = sessions.get_mut(&key) {
                rec.last_seen = now;
            }
            true
        }
        None => false,
    }
}

/// Look a hash up in the long-lived `api_tokens` table.
async fn token_lookup(state: &AppState, hash: &TokenHash, refresh: bool) -> bool {
    let Ok(Some(row)) = state.runtime.store.find_api_token_by_hash(hash).await else {
        return false;
    };
    if row.token_hash.len() != TOKEN_BYTES || bool::from(!row.token_hash.ct_eq(hash)) {
        return false;
    }
    if refresh {
        let now_ms = chrono::Utc::now().timestamp_millis();
        if row
            .last_used
            .is_none_or(|prev| now_ms - prev > TOUCH_INTERVAL_MS)
        {
            let _ = state.runtime.store.touch_api_token(&row.id, now_ms).await;
        }
    }
    true
}

/// Authenticate a presented bearer string.
///
/// Sessions first, then long-lived tokens. Returns the principal — the
/// token's hash — so a caller (the SSE ticket issuer) can bind something
/// to the exact bearer that was presented. `None` is a 401: a token that
/// was never issued, one that expired, and one that was revoked are the
/// same answer.
pub async fn authenticate(state: &AppState, presented: &str) -> Option<TokenHash> {
    let mut bytes = hex::decode(presented).ok()?;
    if bytes.len() != TOKEN_BYTES {
        bytes.zeroize();
        return None;
    }
    let hash = hash_token(&bytes);
    bytes.zeroize();
    if session_lookup(state, &hash, true).await || token_lookup(state, &hash, true).await {
        Some(hash)
    } else {
        None
    }
}

/// Re-check a principal without refreshing it — used when redeeming an
/// SSE ticket, so a logged-out session cannot cash one in.
pub async fn principal_valid(state: &AppState, hash: &TokenHash) -> bool {
    session_lookup(state, hash, false).await || token_lookup(state, hash, false).await
}

/// Pull the bearer out of an `Authorization` header.
pub fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
}

/// `POST /auth/login` body.
#[derive(Deserialize)]
pub struct LoginRequest {
    /// The vault passphrase. Verified, never stored, zeroized here.
    pub passphrase: String,
}

/// POST /auth/login — unauthenticated, rate-limited. Verifies the
/// passphrase and mints a fresh session token.
pub async fn login(State(state): State<AppState>, Json(req): Json<LoginRequest>) -> Response {
    let mut passphrase = req.passphrase;
    let presented = derive_api_token_hash(passphrase.as_bytes());
    passphrase.zeroize();

    // Constant-time compare against the boot-time verifier. This value
    // is the verifier ONLY — `authenticate` never accepts it.
    if bool::from(!presented.ct_eq(&state.api_token_hash)) {
        tracing::warn!("login rejected: passphrase did not verify");
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "invalid passphrase" })),
        )
            .into_response();
    }

    let (token, hash) = mint_token();
    let now = Instant::now();
    let (idle, _) = timeouts(&state);
    state.sessions.lock().await.insert(
        hash,
        SessionRecord {
            issued_at: now,
            last_seen: now,
        },
    );
    tracing::info!("login accepted; new session minted");
    Json(serde_json::json!({ "token": token, "expires_in": idle.as_secs() })).into_response()
}

/// POST /auth/logout — drops the presented session. A long-lived token
/// is not a session; revoke those with `DELETE /auth/tokens/{id}`.
pub async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(presented) = bearer(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Ok(bytes) = hex::decode(presented) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if bytes.len() != TOKEN_BYTES {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let hash = hash_token(&bytes);
    let removed = state.sessions.lock().await.remove(&hash).is_some();
    Json(serde_json::json!({ "logged_out": removed })).into_response()
}

/// `POST /auth/tokens` body.
#[derive(Deserialize)]
pub struct CreateTokenRequest {
    /// User-chosen label, e.g. `springtale-cli@laptop`.
    pub name: String,
}

/// POST /auth/tokens — mint a long-lived named token. Authenticated.
/// The token string is in the response and nowhere else, ever.
pub async fn create_token(
    State(state): State<AppState>,
    Json(req): Json<CreateTokenRequest>,
) -> Response {
    let name = req.name.trim().to_owned();
    if name.is_empty() || name.len() > MAX_TOKEN_NAME_LEN {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "name must be 1..=128 characters" })),
        )
            .into_response();
    }
    let (token, hash) = mint_token();
    let row = ApiTokenRow {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.clone(),
        token_hash: hash.to_vec(),
        created_at: chrono::Utc::now().timestamp_millis(),
        last_used: None,
    };
    let id = row.id.clone();
    if let Err(e) = state.runtime.store.insert_api_token(row).await {
        tracing::error!(error = %e, "failed to store api token");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    tracing::info!(name = %name, "long-lived API token issued");
    Json(serde_json::json!({ "id": id, "name": name, "token": token })).into_response()
}

/// GET /auth/tokens — metadata only. The hash never crosses the wire.
pub async fn list_tokens(State(state): State<AppState>) -> Response {
    match state.runtime.store.list_api_tokens().await {
        Ok(rows) => {
            let tokens: Vec<serde_json::Value> = rows
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "id": r.id,
                        "name": r.name,
                        "created_at": r.created_at,
                        "last_used": r.last_used,
                    })
                })
                .collect();
            Json(serde_json::json!({ "tokens": tokens })).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to list api tokens");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// DELETE /auth/tokens/{id} — revoke. The next request carrying that
/// token fails its lookup, so revocation is immediate.
pub async fn delete_token(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    if let Err(status) = super::validate_path_param(&id) {
        return status.into_response();
    }
    match state.runtime.store.delete_api_token(&id).await {
        Ok(true) => Json(serde_json::json!({ "revoked": true })).into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!(error = %e, "failed to revoke api token");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mint_token_is_unique_and_full_width() {
        let (a, ha) = mint_token();
        let (b, hb) = mint_token();
        assert_eq!(a.len(), TOKEN_BYTES * 2);
        assert_ne!(a, b);
        assert_ne!(ha, hb);
    }

    #[test]
    fn test_hash_token_matches_mint() {
        let (hex_token, hash) = mint_token();
        let bytes = hex::decode(&hex_token).expect("hex");
        assert_eq!(hash_token(&bytes), hash);
    }

    #[test]
    fn test_session_alive_respects_both_timeouts() {
        let now = Instant::now();
        let idle = Duration::from_secs(1800);
        let absolute = Duration::from_secs(43_200);
        let fresh = SessionRecord {
            issued_at: now,
            last_seen: now,
        };
        assert!(session_alive(&fresh, idle, absolute, now));
        let idle_expired = SessionRecord {
            issued_at: now,
            last_seen: now - Duration::from_secs(1801),
        };
        assert!(!session_alive(&idle_expired, idle, absolute, now));
        let absolute_expired = SessionRecord {
            issued_at: now - Duration::from_secs(43_201),
            last_seen: now,
        };
        assert!(!session_alive(&absolute_expired, idle, absolute, now));
    }
}
