use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::{Mutex, broadcast, mpsc};

use springtale_core::rule::engine::TriggerEvent;
use crate::scheduler::AppScheduler;

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
    pub scheduler: AppScheduler,
    /// Rate limit: maximum requests per second for the management API.
    pub rate_limit_per_sec: u64,
    /// Broadcast channel for SSE event streaming to dashboard.
    pub event_tx: broadcast::Sender<springtale_store::schema::events::EventEntry>,
    /// Heartbeat monitor — periodic rule evaluation.
    pub heartbeat_monitor: Arc<Mutex<springtale_scheduler::HeartbeatMonitor>>,
}

impl AppState {
    /// Check if the daemon has completed its boot sequence.
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }
}
