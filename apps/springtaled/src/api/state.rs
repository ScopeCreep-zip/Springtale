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
    /// HMAC-SHA256 hash of the API token (derived from vault passphrase).
    pub api_token_hash: [u8; 32],
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
    pub bot_msg_tx: mpsc::Sender<springtale_bot::IncomingMessage>,
    /// W5 in-app chat: broadcast of bot replies whose origin connector is
    /// `in-app` (the desktop/web/PWA chat panel). The response dispatcher
    /// routes those here instead of to a connector's `send_message`; the
    /// `GET /chat/stream` SSE endpoint fans them out to chat clients.
    pub chat_tx: broadcast::Sender<crate::api::chat::ChatStreamMessage>,
}

impl AppState {
    /// Check if the daemon has completed its boot sequence.
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }
}
