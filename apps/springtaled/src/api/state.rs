use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::{Mutex, broadcast, mpsc};

use springtale_core::rule::engine::TriggerEvent;
use springtale_runtime::EmbeddedScheduler;

/// Shared application state for all API handlers.
///
/// Wraps the shared RuntimeState (same as the desktop app) plus
/// daemon-specific fields (scheduler, job queue, API auth, etc.).
///
/// Cloneable (all fields are `Arc`). Passed to handlers via axum `State<AppState>`.
#[derive(Clone)]
pub struct AppState {
    /// Shared runtime — store, registry, engine, AI, sentinel, canvas.
    /// Same struct used by the desktop app.
    pub runtime: springtale_runtime::RuntimeState,
    /// `HMAC-SHA256(vault passphrase)` — the **login verifier** (6.6).
    /// `POST /auth/login` compares the presented passphrase against this
    /// in constant time. It is never accepted as a bearer token.
    pub api_token_hash: [u8; 32],
    /// Live login sessions, keyed by `sha256(token)` (plan 6.6). In
    /// memory only: locking the vault stops the daemon and every
    /// session with it.
    pub sessions: crate::api::login::SessionMap,
    /// Set to true after the full boot sequence completes.
    pub ready: Arc<AtomicBool>,
    /// Channel for dispatching trigger events to the rule engine event loop.
    pub trigger_tx: mpsc::Sender<TriggerEvent>,
    /// Trigger scheduler — manages cron and filesystem watchers for rules.
    /// Shared with the desktop app via `springtale_runtime::EmbeddedScheduler`.
    pub scheduler: EmbeddedScheduler,
    /// Rate limit: maximum requests per second for the management API.
    pub rate_limit_per_sec: u64,
    /// Broadcast channel for SSE event streaming to dashboard.
    pub event_tx: broadcast::Sender<springtale_store::schema::events::EventEntry>,
    /// Heartbeat monitor — periodic rule evaluation.
    pub heartbeat_monitor: Arc<Mutex<springtale_scheduler::HeartbeatMonitor>>,
    /// Trigger registry — manages ConnectorEvent subscriptions per-rule.
    /// Attach on rule create/enable, detach on rule disable/delete.
    pub trigger_registry: springtale_runtime::TriggerRegistry,
    /// Channel for routing webhook-delivered chat messages to the bot runtime.
    /// Required for Telegram/Discord webhook mode (polling mode uses gateway bridge directly).
    pub bot_msg_tx: mpsc::Sender<springtale_connector::chat::ChatMessage>,
    /// W5 in-app chat: broadcast of bot replies whose origin connector is
    /// `in-app` (the desktop/web/PWA chat panel). The response dispatcher
    /// routes those here instead of to a connector's `send_message`; the
    /// `GET /chat/stream` SSE endpoint fans them out to chat clients.
    pub chat_tx: broadcast::Sender<crate::api::chat::ChatStreamMessage>,
    /// One-time, 30 s tickets for the SSE routes (`GET /stream`,
    /// `GET /chat/stream`). Issued by `POST /stream/ticket` under bearer
    /// auth, consumed by `require_stream_ticket`. Keeps bearer tokens out
    /// of URLs (EventSource cannot send headers; plan 0.7).
    pub stream_tickets: Arc<Mutex<HashMap<String, crate::api::login::StreamTicket>>>,
    /// Flipped to `true` exactly once, when the daemon locks (plan
    /// 6.10). Every SSE handler ends its stream on it.
    ///
    /// Without this a live `GET /stream` would hold an `AppState` clone
    /// — and therefore the store handle and the database key — for as
    /// long as the browser tab stayed open, which would make "locked"
    /// a lie. Closing the streams is also what tells the canvas to
    /// render the locked state (plan 3.6).
    pub lock_signal: tokio::sync::watch::Sender<bool>,
}

impl AppState {
    /// Check if the daemon has completed its boot sequence.
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    /// A future that resolves when the daemon locks.
    ///
    /// SSE handlers pass it to `StreamExt::take_until` so their stream
    /// ends — and releases its `AppState` clone — on lock.
    /// `use<>` — the future owns a `watch::Receiver` and captures
    /// nothing from `self`, so an SSE body may outlive this borrow.
    pub fn locked(&self) -> impl std::future::Future<Output = ()> + Send + use<> {
        let mut rx = self.lock_signal.subscribe();
        async move {
            // `wait_for` returns immediately when the value is already
            // true, and `Err` only when every sender is gone — which
            // itself means the state is being torn down.
            let _ = rx.wait_for(|locked| *locked).await;
        }
    }
}
