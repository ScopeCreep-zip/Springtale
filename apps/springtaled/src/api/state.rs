use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::{Mutex, RwLock, mpsc};

use springtale_connector::registry::store::ConnectorRegistry;
use springtale_core::rule::engine::{RuleEngine, TriggerEvent};
use springtale_scheduler::cron::executor::CronExecutor;
use springtale_scheduler::watcher::fs_watcher::FsWatcher;
use springtale_store::backend::sqlite::SqliteBackend;

/// Shared application state for all API handlers.
///
/// Cloneable (all fields are `Arc`). Passed to handlers via axum `State<AppState>`.
#[derive(Clone)]
pub struct AppState {
    pub store: Arc<SqliteBackend>,
    pub registry: Arc<RwLock<ConnectorRegistry>>,
    pub engine: Arc<RwLock<RuleEngine>>,
    /// HMAC-SHA256 hash of the API token (derived from vault passphrase).
    pub api_token_hash: [u8; 32],
    /// Set to true after the full boot sequence completes.
    pub ready: Arc<AtomicBool>,
    /// Channel for dispatching trigger events to the rule engine event loop.
    pub trigger_tx: mpsc::Sender<TriggerEvent>,
    /// Cron scheduler — needed to schedule/cancel triggers when rules change at runtime.
    pub cron: Arc<Mutex<CronExecutor>>,
    /// File watcher — needed to watch/unwatch paths when rules change at runtime.
    pub fs_watcher: Arc<Mutex<FsWatcher>>,
    /// Rate limit: maximum requests per second for the management API.
    pub rate_limit_per_sec: u64,
}

impl AppState {
    /// Check if the daemon has completed its boot sequence.
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }
}
