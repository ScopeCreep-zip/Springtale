//! Runtime state — shared between springtaled and desktop.
//!
//! Both apps wrap this with their own app-specific fields:
//! - springtaled adds: trigger_tx, cron, fs_watcher, heartbeat, event_tx, api_token_hash
//! - Desktop adds: vault (UI unlock), auto_lock

use std::sync::Arc;

use arc_swap::ArcSwap;
use tokio::sync::{RwLock, broadcast, mpsc};

use springtale_connector::registry::store::ConnectorRegistry;
use springtale_connector::wasm::{WasmEngine, WasmTierCache};
use springtale_core::canvas::{CanvasState, CanvasUpdate};
use springtale_core::rule::engine::RuleEngine;

use crate::cooperation::CapabilityBridge;
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
    /// Process-wide per-tier `InstancePre` cache (§16). Every WASM
    /// connector registers its module against this cache on install;
    /// tier transitions are then a cheap `InstancePre::instantiate`.
    /// Wired to formation momentum via `CapabilityBridge` (Phase 17).
    pub wasm_tier_cache: Arc<WasmTierCache>,
    /// Bridge from formation `MomentumTier` to connector-layer
    /// `WasmTier` + capability-checker-scoped dispatch. Event-loop code
    /// in the bot runtime routes connector calls through this so every
    /// invocation is tagged with the caller's formation tier (§16).
    pub capability_bridge: CapabilityBridge,
    /// Process-wide role registry (§14.4). Pre-populated with
    /// General/Information/Support; connectors register their custom
    /// roles here at install time so formation reload (Phase 3) can
    /// reconstruct members by role name.
    pub role_registry: Arc<springtale_cooperation::role::RoleRegistry>,
    /// Canvas/A2UI state.
    pub canvas: Arc<RwLock<CanvasState>>,
    /// Broadcast channel for canvas updates.
    pub canvas_tx: broadcast::Sender<CanvasUpdate>,
    /// Delivery fan-out for fired `Notify` / `SendMessage` steps
    /// (`crate::notification`). The embedded job consumer walks each
    /// finished chain and broadcasts every user-facing delivery step
    /// here; subscribers forward to the in-app chat stream (daemon SSE
    /// / desktop Tauri event) and a best-effort OS notification.
    /// Mirror of the `canvas_tx` pattern. Without this a scheduled
    /// `Notify`/`SendMessage` recipe fires and the user receives
    /// nothing.
    pub notification_tx: broadcast::Sender<crate::notification::NotificationEvent>,
    /// ConnectorEvent subscription registry (`crate::triggers`). Empty
    /// until `bootstrap_embedded` runs — it owns the `trigger_tx` the
    /// registry needs, so it builds the registry, wires every enabled
    /// ConnectorEvent rule at boot, and `set`s it here. Every deploy
    /// surface (recipe-library apply, chat `RuntimeRecipeDeployer`, rule
    /// CRUD) reaches the SAME registry through this, so a messaging bot
    /// deployed by ANY path attaches its connector-event handler and
    /// actually fires. `OnceLock` because it's written exactly once, at
    /// boot, then read concurrently.
    pub trigger_registry: Arc<std::sync::OnceLock<crate::triggers::TriggerRegistry>>,
    /// Broadcast channel for cooperation events (Phase H1/H2). Each
    /// internal-state cooperation change (intervention fired, sacrifice
    /// yielded, vote opened, role transformed, member marked down,
    /// supervisor escalation, pacing phase change, cascade hit, recovery
    /// action, surface deposit, interference event, CFP round, replan
    /// outcome, commit phase) emits a `CooperationEventEnvelope` here.
    /// Subscribers: SSE handler at `/cooperation/events` (web) and Tauri
    /// `subscribe_cooperation` IPC channel (desktop). Mirror of
    /// `canvas_tx` pattern. Capacity 512 — covers ~30s headroom at 4
    /// formations × 30Hz cadence.
    pub cooperation_tx: broadcast::Sender<springtale_cooperation::CooperationEventEnvelope>,
    /// Formation command sender — runtime operations deploy/dissolve/pause
    /// formations by sending commands to the bot event loop.
    pub formation_cmd_tx: mpsc::Sender<springtale_cooperation::command::FormationCommand>,
    /// Live formation reader — provides enriched per-member data from the
    /// bot event loop. `None` when running as desktop (connects via HTTP).
    pub live_formations: Option<Arc<dyn LiveFormationReader>>,
    /// Shared gossip substrate for cross-formation awareness (§8).
    /// Single-process deployments run an `InMemoryGossipStore`
    /// (`DashMap`); setting `CooperationConfig::cross_process = true`
    /// swaps in `ChitchatGossipStore` at init time.
    pub gossip_store: Arc<dyn springtale_cooperation::awareness::GossipStore>,
    /// G6 — cross-formation gossip bus (`COOPERATION.md §17.2` /
    /// `COOPERATION_IMPLEMENTATION_PLAN.md §12.2`). Per-formation
    /// `FormationView` broadcasts + sticky `FormationOutcome` records.
    /// Defaults to `InMemoryFormationGossipBus`; cross-process
    /// deployments can swap in a chitchat-backed implementation.
    pub formation_gossip: Arc<dyn springtale_cooperation::gossip::FormationGossipBus>,
    /// G2 — global cross-formation knowledge store (`COOPERATION.md §21` /
    /// `COOPERATION_IMPLEMENTATION_PLAN.md §12.6`). Outcomes from every
    /// dissolved formation persist here; new formations seed their
    /// initial mental model from prior outcomes ranked by intent +
    /// connector overlap. Defaults to `PersistentKnowledgeStore` (SQLite-
    /// backed via the `config_store` table); a future Qdrant Edge /
    /// fastembed-rs backend can land behind the same trait.
    pub knowledge_store: Arc<dyn springtale_cooperation::memory::GlobalKnowledgeStore>,
    /// SWIM liveness node (§8.3). `None` for single-process deployments
    /// (no peer processes to probe); `Some(node)` when
    /// `CooperationConfig::cross_process = true` — the node probes peer
    /// processes and emits `SwimEvent`s that awareness consumers
    /// subscribe to.
    pub swim_node: Option<Arc<springtale_cooperation::awareness::SwimNode>>,
}

impl RuntimeState {
    /// Diagnostic: the SWIM node's local bind address when cross-process
    /// mode is active. Returns `None` for single-process deployments
    /// (which never spawn a SWIM node). Exposed so observability /
    /// health-probe surfaces can surface liveness membership info —
    /// also keeps the `swim_node` field honestly read, not just held
    /// to prevent Drop.
    pub fn swim_local_addr(&self) -> Option<std::net::SocketAddr> {
        self.swim_node.as_ref().map(|n| n.local_addr())
    }
}
