//! Runtime state — shared between springtaled and desktop.
//!
//! Both apps wrap this with their own app-specific fields:
//! - springtaled adds: trigger_tx, cron, fs_watcher, heartbeat, event_tx, api_token_hash
//! - Desktop adds: vault (UI unlock), auto_lock

use std::sync::Arc;

use tokio::sync::{RwLock, broadcast};

use springtale_connector::registry::store::ConnectorRegistry;
use springtale_core::canvas::{CanvasState, CanvasUpdate};
use springtale_core::rule::engine::RuleEngine;

/// Shared runtime state — the same regardless of whether it's
/// running as a headless daemon or a desktop app with a GUI.
///
/// All operations in `operations/` take `&RuntimeState` and work
/// with these fields. Both springtaled (HTTP) and desktop (IPC)
/// call the same operation functions with the same state.
///
/// Clone is cheap — all fields are Arc or broadcast::Sender.
/// Required by axum's State<AppState> in springtaled.
#[derive(Clone)]
pub struct RuntimeState {
    /// Persistent storage (SQLite or in-memory).
    pub store: Arc<dyn springtale_store::StorageBackend>,
    /// Connector registry — loaded from store at init.
    pub registry: Arc<RwLock<ConnectorRegistry>>,
    /// Rule engine — loaded from store at init.
    pub engine: Arc<RwLock<RuleEngine>>,
    /// AI adapter (NoopAdapter if none configured).
    pub ai_adapter: Arc<dyn springtale_ai::AiAdapter>,
    /// Sentinel behavioral monitor.
    pub sentinel: Arc<springtale_sentinel::Sentinel>,
    /// Canvas/A2UI state.
    pub canvas: Arc<RwLock<CanvasState>>,
    /// Broadcast channel for canvas updates.
    pub canvas_tx: broadcast::Sender<CanvasUpdate>,
}
