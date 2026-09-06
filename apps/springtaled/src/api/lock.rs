//! The daemon locks (plan 6.10, finding 113).
//!
//! Before this, the only way to protect the vault was to kill
//! `springtaled`. The precedent is the other way round: the Secret
//! Service daemon keeps running with its collections locked, and
//! ssh-agent "should suspend processing of sensitive operations ...
//! until it has been unlocked". Springtale follows Secret Service and
//! *drops the key* rather than fencing it, because the threat model
//! includes memory capture — a boolean that gates handlers while the
//! runtime stays live protects nothing from anyone who can read the
//! process.
//!
//! So locking is not a flag. [`RuntimeGuard`] holds the entire live
//! world — the built inner [`Router`], its [`AppState`], the vault and
//! every background task — behind an [`ArcSwapOption`]. Locking ends
//! the tasks and stores `None`, which drops the last `AppState` clone
//! and with it the `RuntimeState`: the SQLite handle closes, the
//! database key and the vault key zeroize, the flock is released.
//!
//! While locked the router answers exactly three routes — `GET /health`,
//! `GET /ready` and `POST /vault/unlock` — and refuses everything else
//! with `503`. Unlocking re-runs the crypto and runtime initialisation
//! with the supplied passphrase and swaps a freshly built world back in.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwapOption;
use axum::Json;
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Router, middleware};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Deserializer};
use tower::ServiceBuilder;
use tower::ServiceExt;
use tower::buffer::BufferLayer;
use tower::limit::RateLimitLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;

use springtale_crypto::vault::store::Vault;

use super::state::AppState;

/// How often the auto-lock timer checks the idle clock.
const AUTO_LOCK_POLL: Duration = Duration::from_secs(5);

/// Unlock attempts allowed per minute. Excess attempts queue behind the
/// rate limiter and time out rather than reaching the KDF.
const UNLOCK_ATTEMPTS_PER_MINUTE: u64 = 5;

/// Body cap for `POST /vault/unlock`. A passphrase is not a payload.
const UNLOCK_BODY_LIMIT: usize = 4096;

/// How long [`RuntimeGuard::lock`] waits for in-flight requests to let
/// go of the runtime before giving up and letting them drop it.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

/// One poll of the drain wait.
const DRAIN_POLL: Duration = Duration::from_millis(20);

/// Cooperative yields to spend waiting for the store handle to close
/// before falling back to timed polling.
const YIELDS_AFTER_DROP: usize = 32;

/// Everything an unlocked daemon owns.
///
/// Held behind a single `Arc` inside [`RuntimeGuard`], so replacing that
/// `Arc` with `None` is what "locked" means. Nothing here is reachable
/// while locked, and nothing here survives a lock.
pub struct Live {
    /// The real management API, already built over `state`.
    router: Router,
    /// The same state `router` was built over. Locking reads the
    /// scheduler, the chat wiring and the token hash out of it.
    state: AppState,
    /// Daemon-side background tasks (bot event loop, response
    /// dispatcher, notification forwarder, retention purge). The
    /// runtime's own tasks live on `state.runtime.tasks`.
    daemon_tasks: springtale_runtime::TaskHandles,
    /// The open vault. `None` once locked — [`Vault::lock`] zeroizes
    /// the key material, and the drop that follows releases the file.
    vault: std::sync::Mutex<Option<Vault>>,
    /// Bound transport (Unix socket / mTLS HTTP). Held only so it is
    /// dropped with everything else. `None` when the caller bound none
    /// — the in-memory test fixture, which has no socket to own.
    _transport: Option<Arc<dyn springtale_transport::Transport>>,
}

impl Live {
    /// Build the inner router over `state` and take ownership of
    /// everything a lock has to tear down.
    pub fn new(
        state: AppState,
        daemon_tasks: springtale_runtime::TaskHandles,
        vault: Vault,
        transport: Option<Arc<dyn springtale_transport::Transport>>,
    ) -> Self {
        Self {
            router: super::build_router(state.clone()),
            state,
            daemon_tasks,
            vault: std::sync::Mutex::new(Some(vault)),
            _transport: transport,
        }
    }

    /// The state the inner router was built over.
    pub fn state(&self) -> &AppState {
        &self.state
    }

    /// Take the vault out so it can be locked and dropped.
    fn take_vault(&self) -> Option<Vault> {
        match self.vault.lock() {
            Ok(mut slot) => slot.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        }
    }
}

/// What [`Rebuild`] returns: a fresh [`Live`], or the reason it could
/// not be built (a wrong passphrase, most often).
pub type RebuildFuture = Pin<Box<dyn Future<Output = anyhow::Result<Live>> + Send>>;

/// Re-runs the boot pipeline with a caller-supplied passphrase.
///
/// The daemon's implementation is
/// `crate::runtime::boot::pipeline::build_live`; tests substitute their
/// own so the lock/unlock cycle can be exercised without a vault file.
pub type Rebuild = Arc<dyn Fn(SecretString) -> RebuildFuture + Send + Sync>;

/// The lock. Holds the live world, or nothing at all.
#[derive(Clone)]
pub struct RuntimeGuard {
    live: Arc<ArcSwapOption<Live>>,
    rebuild: Rebuild,
    /// Serializes lock against unlock, so two `POST /vault/unlock`
    /// racing each other cannot build two runtimes over one database.
    gate: Arc<tokio::sync::Mutex<()>>,
}

impl RuntimeGuard {
    /// Wrap a freshly booted world.
    pub fn new(live: Live, rebuild: Rebuild) -> Self {
        Self {
            live: Arc::new(ArcSwapOption::from(Some(Arc::new(live)))),
            rebuild,
            gate: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    /// The live world, or `None` while locked.
    pub fn live(&self) -> Option<Arc<Live>> {
        self.live.load_full()
    }

    /// Whether the daemon is locked.
    pub fn is_locked(&self) -> bool {
        self.live.load().is_none()
    }

    /// Lock: end everything, then drop the runtime.
    ///
    /// Returns `false` if it was already locked. The order matters —
    /// each step releases a different holder of the `RuntimeState`, and
    /// the drop at the end only frees the key if none are left:
    ///
    /// 1. signal the SSE streams, which hold `AppState` per connection
    /// 2. unwire every connector chat loop
    /// 3. pause the scheduler and stop the heartbeat
    /// 4. clear the session map (one-time stream tickets)
    /// 5. abort and *join* every background task
    /// 6. zeroize the vault key
    /// 7. drop the last `Live`
    pub async fn lock(&self) -> bool {
        let _serialized = self.gate.lock().await;
        let Some(live) = self.live.swap(None) else {
            return false;
        };

        // 1 — SSE first: a subscribed browser tab would otherwise hold
        // an `AppState` clone for as long as it stayed open.
        let _ = live.state.lock_signal.send(true);

        // 2 — every connector's chat receive loop drains and exits.
        springtale_runtime::operations::connectors::unwire_all_chat(&live.state.runtime);

        // 3 — no rule fires while the vault is closed.
        live.state.scheduler.pause().await;
        live.state.heartbeat_monitor.lock().await.stop();

        // 4 — sessions: the login session map and the one-time SSE
        // tickets both die here, so a token issued before the lock
        // cannot be replayed after it. The stored per-user/per-channel
        // bot sessions go with the database handle that closes in step 7.
        live.state.sessions.lock().await.clear();
        live.state.stream_tickets.lock().await.clear();

        // The one observable proof that the lock worked: when the last
        // `Arc` to the store is gone, SQLite is closed and the database
        // key is zeroized. Taken before anything is dropped.
        let store = Arc::downgrade(&live.state.runtime.store);

        // 5 — `shutdown` aborts AND awaits, which is the only way to
        // know the tasks released their `Arc` clones of the store.
        let runtime_tasks = live.state.runtime.tasks.clone();
        let runtime_aborted = runtime_tasks.shutdown().await;
        let daemon_aborted = live.daemon_tasks.shutdown().await;

        // 6 — zeroize the vault key even if step 7 has to wait.
        if let Some(mut vault) = live.take_vault() {
            springtale_runtime::operations::vault::lock_vault(&mut vault);
            drop(vault);
        }

        // 7 — the drop that closes SQLite and zeroizes the database key.
        let drained = drop_live(live).await;

        // 8 — and wait for it to actually happen. Dropping the router
        // does not release its `AppState` synchronously: `tower`'s
        // `Buffer` — the layer fronting the rate limiter — runs a
        // worker task that owns the service, and a chain of such tasks
        // only lets go as each is polled once more. Waiting on the
        // store's `Weak` is the difference between reporting "locked"
        // and being locked.
        let store_released = await_store_release(&store).await;

        tracing::info!(
            runtime_tasks_aborted = runtime_aborted,
            daemon_tasks_aborted = daemon_aborted,
            drained,
            store_released,
            "daemon locked — runtime dropped, vault key zeroized"
        );
        true
    }

    /// Unlock: rebuild the world from `passphrase`.
    ///
    /// Runs the same pipeline boot runs — vault open, key derivation,
    /// `springtale_runtime::init`, scheduler, bot, router — so an
    /// unlocked daemon is indistinguishable from a freshly booted one.
    pub async fn unlock(&self, passphrase: SecretString) -> anyhow::Result<()> {
        let _serialized = self.gate.lock().await;
        if self.live.load().is_some() {
            anyhow::bail!("already unlocked");
        }
        let live = (self.rebuild)(passphrase).await?;
        self.live.store(Some(Arc::new(live)));
        tracing::info!("daemon unlocked — runtime rebuilt");
        Ok(())
    }

    /// Spawn the idle auto-lock timer.
    ///
    /// The interval is `bot:settings.auto_lock_secs` (default 300, `0`
    /// disables), read on every tick so a settings change takes effect
    /// without a restart. "Idle" is `RuntimeState::activity`: no
    /// authenticated request and no inbound chat message.
    pub fn spawn_auto_lock(&self) -> tokio::task::JoinHandle<()> {
        let guard = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(AUTO_LOCK_POLL);
            ticker.tick().await; // the immediate first tick
            loop {
                ticker.tick().await;
                let Some((timeout_secs, idle_secs)) = guard.live().map(|live| {
                    (
                        live.state.runtime.bot_settings.load().auto_lock_secs,
                        live.state.runtime.activity.idle_secs(),
                    )
                }) else {
                    continue; // already locked
                };
                if timeout_secs == 0 || idle_secs < timeout_secs {
                    continue;
                }
                tracing::info!(
                    idle_secs,
                    auto_lock_secs = timeout_secs,
                    "auto-lock: idle timeout reached"
                );
                guard.lock().await;
            }
        })
    }
}

/// Drop the last `Live`, waiting briefly for in-flight requests.
///
/// Returns whether this call was the one that dropped it. A request
/// still executing inside the inner router holds an `Arc<Live>`; the
/// wait gives it a moment to finish, because the very next thing an
/// unlock does is take an exclusive lock on the same database.
async fn drop_live(live: Arc<Live>) -> bool {
    let mut live = live;
    let deadline = tokio::time::Instant::now() + DRAIN_TIMEOUT;
    loop {
        match Arc::try_unwrap(live) {
            Ok(owned) => {
                drop(owned);
                return true;
            }
            Err(still_shared) => {
                if tokio::time::Instant::now() >= deadline {
                    tracing::warn!(
                        holders = Arc::strong_count(&still_shared),
                        "locked with requests still in flight — the runtime drops when they finish"
                    );
                    return false;
                }
                live = still_shared;
                tokio::time::sleep(DRAIN_POLL).await;
            }
        }
    }
}

/// Wait until the last `Arc` to the store is gone.
///
/// Returns whether it closed within [`DRAIN_TIMEOUT`]. A `false` is
/// worth shouting about: it means something still holds the database
/// open — and therefore its key — after a lock.
async fn await_store_release(
    store: &std::sync::Weak<dyn springtale_store::StorageBackend>,
) -> bool {
    let deadline = tokio::time::Instant::now() + DRAIN_TIMEOUT;
    let mut spins = 0usize;
    loop {
        if store.strong_count() == 0 {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            tracing::error!(
                holders = store.strong_count(),
                "vault locked but the store handle is still open — key not released"
            );
            return false;
        }
        // Yield first: the holders are usually tasks waiting to be
        // polled once, and yielding runs them immediately.
        if spins < YIELDS_AFTER_DROP {
            tokio::task::yield_now().await;
        } else {
            tokio::time::sleep(DRAIN_POLL).await;
        }
        spins += 1;
    }
}

/// Build the outer router: three routes that always answer, and a
/// fallback that forwards to the live router or refuses.
pub fn build_outer_router(guard: RuntimeGuard) -> Router {
    // `GET /health` is the same liveness probe either way — the process
    // is up. `GET /ready` reports the lock state, and forwards to the
    // real readiness check (which touches the store) when unlocked.
    let always_on = Router::new()
        .route("/health", get(super::health::health))
        .route("/ready", get(ready));

    // Rate-limited so the Argon2id KDF cannot be driven by a flood of
    // guesses. `BufferLayer` fronts it because `tower::limit::RateLimit`
    // is not `Clone`; the same pairing the main router uses.
    let unlock_route = Router::new().route("/vault/unlock", post(unlock)).layer(
        ServiceBuilder::new()
            .layer(axum::error_handling::HandleErrorLayer::new(
                |_err: tower::BoxError| async move { StatusCode::TOO_MANY_REQUESTS },
            ))
            .layer(BufferLayer::new(16))
            .layer(RateLimitLayer::new(
                UNLOCK_ATTEMPTS_PER_MINUTE,
                Duration::from_secs(60),
            )),
    );

    let vault_routes = Router::new()
        .route("/vault/lock", post(lock))
        .merge(unlock_route)
        .layer(DefaultBodyLimit::max(UNLOCK_BODY_LIMIT))
        .layer(RequestBodyLimitLayer::new(UNLOCK_BODY_LIMIT))
        .layer(middleware::from_fn(super::auth::require_csrf_protection));

    let router = Router::new()
        .merge(always_on)
        .merge(vault_routes)
        .fallback(forward);

    super::security_headers(router)
        .layer(TraceLayer::new_for_http())
        .with_state(guard)
}

/// Everything that is not one of the always-available routes.
///
/// Unlocked: hand the request to the real router. Locked: `503`.
async fn forward(State(guard): State<RuntimeGuard>, request: Request) -> Response {
    let Some(live) = guard.live() else {
        return locked_refusal();
    };
    let credentialed = is_credentialed(request.headers(), request.uri().query());
    // Clone the router and let go of the `Live` before awaiting, so a
    // concurrent lock is not blocked by this request's `Arc`.
    let router = live.router.clone();
    let activity = live.state.runtime.activity.clone();
    drop(live);
    let response = match router.oneshot(request).await {
        Ok(response) => response,
        // `Router`'s `Service` error type is `Infallible`.
        Err(never) => match never {},
    };
    if credentialed && was_accepted(response.status()) {
        activity.touch();
    }
    response
}

/// Whether a request even claims a credential.
///
/// A bearer token or a stream ticket both mean a person (or their
/// dashboard) is driving the daemon. An unauthenticated probe — a
/// health check, a scan — does not keep the vault open.
fn is_credentialed(headers: &HeaderMap, query: Option<&str>) -> bool {
    if headers.contains_key("authorization") {
        return true;
    }
    query.is_some_and(|q| q.split('&').any(|p| p.starts_with("ticket=")))
}

/// Whether the auth middleware let the request through.
///
/// Checked *after* the fact so a garbage `Authorization` header cannot
/// hold the vault open: anyone on loopback could otherwise defeat
/// auto-lock by spamming rejected requests.
fn was_accepted(status: StatusCode) -> bool {
    status != StatusCode::UNAUTHORIZED && status != StatusCode::FORBIDDEN
}

/// The refusal every non-exempt route gets while locked.
fn locked_refusal() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({
            "error": "vault is locked",
            "locked": true,
        })),
    )
        .into_response()
}

/// GET /ready — the lock state, and the real readiness check under it.
async fn ready(State(guard): State<RuntimeGuard>, request: Request) -> Response {
    let Some(live) = guard.live() else {
        return (
            StatusCode::OK,
            Json(serde_json::json!({ "status": "locked", "locked": true })),
        )
            .into_response();
    };
    let router = live.router.clone();
    drop(live);
    match router.oneshot(request).await {
        Ok(response) => response,
        Err(never) => match never {},
    }
}

/// POST /vault/lock — authenticated.
///
/// Idempotent: locking a locked daemon is a `200`, not an error, so a
/// panic-button UI never has to reason about the current state.
async fn lock(State(guard): State<RuntimeGuard>, headers: HeaderMap) -> Response {
    let Some(live) = guard.live() else {
        return (StatusCode::OK, Json(serde_json::json!({ "locked": true }))).into_response();
    };
    // Same session check `api::auth::require_auth` makes, run by hand:
    // `POST /vault/lock` is served by the outer router, which has no
    // `AppState` for that middleware to sit over. A passphrase-derived
    // hash is the login verifier, never a bearer (plan 6.6), so the
    // presented token has to be looked up in the live session map.
    let Some(token) = super::login::bearer(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if super::login::authenticate(&live.state, token)
        .await
        .is_none()
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    // Release before locking — `lock` waits for exactly this `Arc`.
    drop(live);

    guard.lock().await;
    (StatusCode::OK, Json(serde_json::json!({ "locked": true }))).into_response()
}

/// Body of `POST /vault/unlock`.
#[derive(Deserialize)]
pub struct UnlockRequest {
    /// The vault passphrase. Never logged, never echoed.
    #[serde(deserialize_with = "deserialize_passphrase")]
    passphrase: SecretString,
}

/// Wrap the deserialized passphrase in `SecretString` immediately, so
/// it is zeroized on drop and redacted in any `Debug` output.
fn deserialize_passphrase<'de, D>(deserializer: D) -> Result<SecretString, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    Ok(SecretString::new(raw.into_boxed_str()))
}

/// POST /vault/unlock — public, rate-limited.
///
/// Deliberately unauthenticated: the API token is derived from the
/// passphrase, so there is no credential to present while locked. The
/// passphrase itself is the credential, and `Vault::open` is the check
/// — Argon2id over the wrong passphrase fails at AEAD decryption, with
/// no comparison this code could shortcut.
async fn unlock(State(guard): State<RuntimeGuard>, Json(body): Json<UnlockRequest>) -> Response {
    if !guard.is_locked() {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "already unlocked", "locked": false })),
        )
            .into_response();
    }
    match guard.unlock(body.passphrase).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "locked": false }))).into_response(),
        Err(e) => {
            // The error text describes the failure mode (wrong
            // passphrase, unreadable vault) and never carries the
            // passphrase itself.
            tracing::warn!(error = %e, "vault unlock refused");
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "unlock failed", "locked": true })),
            )
                .into_response()
        }
    }
}

/// The passphrase bytes, for the one call site that needs them.
///
/// SECURITY: expose needed to hand the passphrase to Argon2id in
/// `Vault::open` and to the two HMAC key derivations. The borrow does
/// not outlive the call, and the `SecretString` zeroizes on drop.
pub fn expose_passphrase(passphrase: &SecretString) -> &[u8] {
    passphrase.expose_secret().as_bytes()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn test_is_credentialed_counts_tokens_and_tickets_only() {
        let mut bearer = HeaderMap::new();
        bearer.insert("authorization", HeaderValue::from_static("Bearer abcd"));
        assert!(is_credentialed(&bearer, None));
        assert!(is_credentialed(&HeaderMap::new(), Some("ticket=abcd")));
        assert!(is_credentialed(&HeaderMap::new(), Some("x=1&ticket=ab")));

        assert!(!is_credentialed(&HeaderMap::new(), None));
        assert!(!is_credentialed(&HeaderMap::new(), Some("noticket=ab")));
    }

    #[test]
    fn test_rejected_requests_do_not_count_as_activity() {
        assert!(!was_accepted(StatusCode::UNAUTHORIZED));
        assert!(!was_accepted(StatusCode::FORBIDDEN));
        assert!(was_accepted(StatusCode::OK));
        assert!(was_accepted(StatusCode::NOT_FOUND));
    }
}
