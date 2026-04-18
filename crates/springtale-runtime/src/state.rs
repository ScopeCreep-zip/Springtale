//! Runtime state — shared between springtaled and desktop.
//!
//! Both apps wrap this with their own app-specific fields:
//! - springtaled adds: trigger_tx, cron, fs_watcher, heartbeat, event_tx, api_token_hash
//! - Desktop adds: vault (UI unlock), auto_lock

use std::sync::Arc;

use arc_swap::ArcSwap;
use tokio::sync::{RwLock, broadcast, mpsc};

use springtale_connector::registry::store::ConnectorRegistry;
use springtale_connector::wasm::WasmEngine;
use springtale_core::canvas::{CanvasState, CanvasUpdate};
use springtale_core::rule::engine::RuleEngine;

use crate::operations::formations::FormationMemberDetail;

/// Live formation reader — provides enriched per-member data from the
/// bot event loop's in-memory formations.
///
/// The daemon (springtaled) implements this by reading
/// `Arc<RwLock<Vec<Formation>>>` from the bot runtime. The desktop app
/// passes `None` because it connects to the daemon via HTTP — it doesn't
/// run a bot event loop in-process.
#[async_trait::async_trait]
pub trait LiveFormationReader: Send + Sync {
    /// Read enriched member details for a specific formation.
    ///
    /// Returns an empty vec if the formation is not found in-memory
    /// (e.g., draft formations that haven't been deployed yet).
    async fn read_member_details(&self, formation_id: &str) -> Vec<FormationMemberDetail>;
}

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
    /// Wrapped in ArcSwap for lock-free reads + atomic hot-swap at runtime.
    /// Reads: `state.ai_adapter.load()` — zero contention.
    /// Writes: `state.ai_adapter.store(new_adapter)` — atomic swap.
    pub ai_adapter: Arc<ArcSwap<Arc<dyn springtale_ai::AiAdapter>>>,
    /// Sentinel behavioral monitor.
    pub sentinel: Arc<springtale_sentinel::Sentinel>,
    /// Shared WASM engine for community connectors.
    /// Epoch ticker calls `increment_epoch()` on this for wall-clock timeouts.
    pub wasm_engine: Arc<WasmEngine>,
    /// Canvas/A2UI state.
    pub canvas: Arc<RwLock<CanvasState>>,
    /// Broadcast channel for canvas updates.
    pub canvas_tx: broadcast::Sender<CanvasUpdate>,
    /// Formation command sender — runtime operations deploy/dissolve/pause
    /// formations by sending commands to the bot event loop.
    pub formation_cmd_tx: mpsc::Sender<springtale_cooperation::command::FormationCommand>,
    /// Live formation reader — provides enriched per-member data from the
    /// bot event loop. `None` when running as desktop (connects via HTTP).
    pub live_formations: Option<Arc<dyn LiveFormationReader>>,
}
