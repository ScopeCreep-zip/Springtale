use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use tokio::sync::{RwLock, broadcast, mpsc};

use springtale_connector::registry::store::ConnectorRegistry;
use springtale_core::rule::engine::{RuleEngine, TriggerEvent};
use springtale_store::StorageBackend;

use springtale_cooperation::command::FormationCommand;

use crate::cooperation::cadence::{CadenceBus, Tick, TickReport};
use crate::cooperation::formation::Formation;
use crate::error::BotError;
use crate::handler::HandlerRegistry;
use crate::memory::ConversationContext;
use crate::router::Router;
use crate::state::persona::BotPersona;

/// Incoming message from any chat connector.
#[derive(Debug, Clone)]
pub struct IncomingMessage {
    pub user_id: String,
    pub channel_id: String,
    pub text: String,
    /// Which connector this came from (for reply routing).
    pub source_connector: String,
    /// Raw connector payload.
    pub raw: serde_json::Value,
}

/// Outgoing response to be sent via a connector.
#[derive(Debug, Clone)]
pub struct OutgoingResponse {
    pub channel_id: String,
    pub text: String,
    pub connector: String,
}

/// Configuration for the bot runtime.
#[derive(Debug, Clone, Deserialize)]
pub struct BotConfig {
    /// Conversation context window size. Default: 50.
    #[serde(default = "default_context_window")]
    pub context_window: usize,
    /// Bot persona configuration.
    #[serde(default)]
    pub persona: BotPersona,
    /// Vault auto-lock timeout in seconds. Default: 300 (5 min).
    #[serde(default = "default_vault_timeout")]
    pub vault_timeout_secs: u64,
    /// Controls which connector actions the AI can call as tools.
    /// Default: empty allow list = AI has zero tools (OWASP LLM06).
    #[serde(default)]
    pub tool_policy: springtale_ai::ToolPolicy,
}

fn default_context_window() -> usize {
    50
}

fn default_vault_timeout() -> u64 {
    300
}

impl Default for BotConfig {
    fn default() -> Self {
        Self {
            context_window: default_context_window(),
            persona: BotPersona::default(),
            vault_timeout_secs: default_vault_timeout(),
            tool_policy: springtale_ai::ToolPolicy::default(),
        }
    }
}

/// The bot runtime. Owns all bot subsystems.
pub struct Bot {
    pub(crate) router: Router,
    pub(crate) handlers: HandlerRegistry,
    pub(crate) store: Arc<dyn StorageBackend>,
    pub(crate) registry: Arc<RwLock<ConnectorRegistry>>,
    pub(crate) engine: Arc<RwLock<RuleEngine>>,
    pub(crate) config: BotConfig,
    pub(crate) context: ConversationContext,
    pub(crate) ai_adapter: Arc<dyn springtale_ai::AiAdapter>,
    pub(crate) sentinel: Arc<springtale_sentinel::Sentinel>,
    pub(crate) connector_rx: mpsc::Receiver<IncomingMessage>,
    pub(crate) rule_rx: mpsc::Receiver<TriggerEvent>,
    pub(crate) response_tx: mpsc::Sender<OutgoingResponse>,
    /// Active formations (cooperation module).
    pub(crate) formations: Arc<RwLock<Vec<Formation>>>,
    /// Cadence bus — wrapped in Arc per spec §5.4 pattern since the bus
    /// is shared across the spawned run task and the event loop.
    pub(crate) cadence: Arc<CadenceBus>,
    /// Receiver for cadence ticks (event loop select! branch).
    pub(crate) cadence_rx: broadcast::Receiver<Tick>,
    /// Fan-in receiver for agent tick reports (spec §5.4). Agents send
    /// TickReports through `cadence.reports_sender()`; the event loop drains
    /// this receiver during tick processing.
    pub(crate) cadence_reports_rx: tokio::sync::mpsc::Receiver<TickReport>,
    /// Receiver for formation commands from runtime operations.
    pub(crate) formation_cmd_rx: mpsc::Receiver<FormationCommand>,
    /// Gossip substrate shared across every formation this bot spawns
    /// (per spec §8). Single-process deployments inject
    /// `InMemoryGossipStore`; cross-process deployments inject
    /// `ChitchatGossipStore`. Defaults to the in-memory variant if the
    /// builder doesn't override.
    pub(crate) gossip_store: Arc<dyn springtale_cooperation::awareness::GossipStore>,
    /// Shared cooperation role registry (§14.4 / Phase 21). Built-ins
    /// plus any community roles contributed by installed connectors.
    /// Role transformations in the tick loop look up the target role
    /// by name here so community roles declared in connector manifests
    /// can actually materialize on agents.
    pub(crate) role_registry: Arc<springtale_cooperation::role::RoleRegistry>,
    /// Shared capability bridge (§16 / Phase 17). The single dispatch
    /// point for every connector invocation this bot triggers. Holds a
    /// reference to the process-wide registry and bakes in the tier
    /// scoping that `dispatch_action_with_tier` uses.
    pub(crate) capability_bridge: springtale_runtime::CapabilityBridge,
    /// L6 commander-override evaluator (`COOPERATION.md §3.4`). Pure rule
    /// table — runs every tick over `InterventionSignals` and decides
    /// whether to fire `ChangeIntent / InjectFuel / ForcedDissolve /
    /// EscalateToUser`. The dispatch path lives in
    /// `tick_steps/check_interventions.rs`.
    pub(crate) intervention_evaluator:
        crate::orchestrator::intervention::evaluator::RuleBasedEvaluator,
    /// L6 commander-override executor — applies whichever variant the
    /// evaluator returned to the formation.
    pub(crate) intervention_action:
        crate::orchestrator::intervention::action::DefaultInterventionAction,
    /// F4: shared `runtime.canvas_tx` broadcast — `tick_steps/emit_canvas_update.rs`
    /// publishes per-tick formation summaries here. `None` in tests / headless
    /// builds; production wires it from `RuntimeState::canvas_tx`.
    pub(crate) canvas_tx:
        Option<tokio::sync::broadcast::Sender<springtale_core::canvas::CanvasUpdate>>,
    /// Phase H2: shared `runtime.cooperation_tx` broadcast — every internal-
    /// state cooperation event publishes a `CooperationEventEnvelope` here.
    /// `None` in tests / headless builds; production wires it from
    /// `RuntimeState::cooperation_tx`.
    pub(crate) cooperation_tx: Option<
        tokio::sync::broadcast::Sender<springtale_cooperation::CooperationEventEnvelope>,
    >,
}

impl Bot {
    /// Start the bot event loop. Runs until all channels are closed.
    pub async fn start(mut self) {
        crate::runtime::event_loop::run_event_loop(&mut self).await;
    }
}

/// Builder for constructing a `Bot` instance.
pub struct BotBuilder {
    store: Option<Arc<dyn StorageBackend>>,
    registry: Option<Arc<RwLock<ConnectorRegistry>>>,
    engine: Option<Arc<RwLock<RuleEngine>>>,
    config: BotConfig,
    ai_adapter: Option<Arc<dyn springtale_ai::AiAdapter>>,
    sentinel: Option<Arc<springtale_sentinel::Sentinel>>,
    connector_rx: Option<mpsc::Receiver<IncomingMessage>>,
    rule_rx: Option<mpsc::Receiver<TriggerEvent>>,
    response_tx: Option<mpsc::Sender<OutgoingResponse>>,
    formation_cmd_rx: Option<mpsc::Receiver<FormationCommand>>,
    /// Pre-created formations handle — allows the caller to retain an
    /// `Arc<RwLock<Vec<Formation>>>` that the built `Bot` will use. This
    /// is how the daemon wires `BotFormationReader` to read live data.
    formations_handle: Option<Arc<RwLock<Vec<Formation>>>>,
    /// Injected gossip substrate (defaults to `InMemoryGossipStore`).
    gossip_store: Option<Arc<dyn springtale_cooperation::awareness::GossipStore>>,
    /// Injected role registry — REQUIRED. Callers must pass the
    /// process-wide `RuntimeState::role_registry` so there is exactly
    /// one registry holding built-ins plus community roles declared in
    /// connector manifests (§14.4 / Phase 21). Silently making up a
    /// fresh registry would let community roles registered in
    /// `RuntimeState` be invisible to the bot's tick loop.
    role_registry: Option<Arc<springtale_cooperation::role::RoleRegistry>>,
    /// Injected capability bridge — REQUIRED. Callers must pass the
    /// process-wide `RuntimeState::capability_bridge`. Silently
    /// constructing a fresh bridge from `registry` creates a second
    /// dispatch object; audit rules call out that as a fallback
    /// duplication even though both wrap the same `Arc`. Enforce one
    /// shared instance.
    capability_bridge: Option<springtale_runtime::CapabilityBridge>,
    /// F4: optional canvas broadcast sender. The daemon plumbs in
    /// `RuntimeState::canvas_tx`; tests + headless leave None.
    canvas_tx: Option<tokio::sync::broadcast::Sender<springtale_core::canvas::CanvasUpdate>>,
    /// Phase H2: optional cooperation events broadcast sender. The daemon
    /// plumbs in `RuntimeState::cooperation_tx`; tests + headless leave None.
    cooperation_tx: Option<
        tokio::sync::broadcast::Sender<springtale_cooperation::CooperationEventEnvelope>,
    >,
}

impl BotBuilder {
    pub fn new() -> Self {
        Self {
            store: None,
            registry: None,
            engine: None,
            config: BotConfig::default(),
            ai_adapter: None,
            sentinel: None,
            connector_rx: None,
            rule_rx: None,
            response_tx: None,
            formation_cmd_rx: None,
            formations_handle: None,
            gossip_store: None,
            role_registry: None,
            capability_bridge: None,
            canvas_tx: None,
            cooperation_tx: None,
        }
    }

    /// F4: inject the runtime's canvas broadcast sender so the bot tick
    /// loop's `emit_canvas_update` step publishes live formation state.
    pub fn canvas_tx(
        mut self,
        tx: tokio::sync::broadcast::Sender<springtale_core::canvas::CanvasUpdate>,
    ) -> Self {
        self.canvas_tx = Some(tx);
        self
    }

    /// Phase H2: inject the runtime's cooperation events broadcast sender so
    /// every internal-state cooperation event reaches subscribers via the
    /// SSE endpoint `/cooperation/events` (web) and the Tauri
    /// `subscribe_cooperation` IPC channel (desktop).
    pub fn cooperation_tx(
        mut self,
        tx: tokio::sync::broadcast::Sender<springtale_cooperation::CooperationEventEnvelope>,
    ) -> Self {
        self.cooperation_tx = Some(tx);
        self
    }

    /// Inject a gossip substrate. Defaults to a fresh `InMemoryGossipStore`
    /// if not set — the daemon (runtime::init) injects a process-shared
    /// `ChitchatGossipStore` for cross-process federations.
    pub fn gossip_store(
        mut self,
        store: Arc<dyn springtale_cooperation::awareness::GossipStore>,
    ) -> Self {
        self.gossip_store = Some(store);
        self
    }

    /// Inject the shared role registry. The daemon should pass
    /// `RuntimeState.role_registry` so community roles contributed by
    /// connector manifests are reachable from the tick loop's role
    /// transformation path.
    pub fn role_registry(
        mut self,
        registry: Arc<springtale_cooperation::role::RoleRegistry>,
    ) -> Self {
        self.role_registry = Some(registry);
        self
    }

    /// Inject the shared capability bridge. The daemon should pass
    /// `RuntimeState.capability_bridge` so every connector invocation
    /// across this bot flows through the same dispatch point.
    pub fn capability_bridge(
        mut self,
        bridge: springtale_runtime::CapabilityBridge,
    ) -> Self {
        self.capability_bridge = Some(bridge);
        self
    }

    /// Create and return a shared formations handle.
    ///
    /// The caller retains the `Arc` to build a `LiveFormationReader` from it.
    /// When `build()` is called, the bot will use this handle instead of
    /// creating its own. This allows the daemon to read live formation data
    /// without locking or duplicating state.
    pub fn create_formations_handle(&mut self) -> Arc<RwLock<Vec<Formation>>> {
        let handle = Arc::new(RwLock::new(Vec::new()));
        self.formations_handle = Some(handle.clone());
        handle
    }

    pub fn store(mut self, store: Arc<dyn StorageBackend>) -> Self {
        self.store = Some(store);
        self
    }

    pub fn registry(mut self, registry: Arc<RwLock<ConnectorRegistry>>) -> Self {
        self.registry = Some(registry);
        self
    }

    pub fn engine(mut self, engine: Arc<RwLock<RuleEngine>>) -> Self {
        self.engine = Some(engine);
        self
    }

    pub fn config(mut self, config: BotConfig) -> Self {
        self.config = config;
        self
    }

    pub fn connector_rx(mut self, rx: mpsc::Receiver<IncomingMessage>) -> Self {
        self.connector_rx = Some(rx);
        self
    }

    pub fn rule_rx(mut self, rx: mpsc::Receiver<TriggerEvent>) -> Self {
        self.rule_rx = Some(rx);
        self
    }

    pub fn response_tx(mut self, tx: mpsc::Sender<OutgoingResponse>) -> Self {
        self.response_tx = Some(tx);
        self
    }

    pub fn ai_adapter(mut self, adapter: Arc<dyn springtale_ai::AiAdapter>) -> Self {
        self.ai_adapter = Some(adapter);
        self
    }

    pub fn sentinel(mut self, sentinel: Arc<springtale_sentinel::Sentinel>) -> Self {
        self.sentinel = Some(sentinel);
        self
    }

    pub fn formation_cmd_rx(mut self, rx: mpsc::Receiver<FormationCommand>) -> Self {
        self.formation_cmd_rx = Some(rx);
        self
    }

    /// Set a pre-created shared formations handle.
    ///
    /// When set, `build()` uses this Arc instead of creating a fresh one.
    /// The caller retains a clone to build a `LiveFormationReader` from it.
    pub fn formations_handle(mut self, handle: Arc<RwLock<Vec<Formation>>>) -> Self {
        self.formations_handle = Some(handle);
        self
    }

    /// Build the bot, initializing router, handlers, and context.
    pub async fn build(self) -> Result<Bot, BotError> {
        let store = self
            .store
            .ok_or_else(|| BotError::NotInitialized("store required".into()))?;
        let registry = self
            .registry
            .ok_or_else(|| BotError::NotInitialized("registry required".into()))?;
        let engine = self
            .engine
            .ok_or_else(|| BotError::NotInitialized("engine required".into()))?;
        let connector_rx = self
            .connector_rx
            .ok_or_else(|| BotError::NotInitialized("connector_rx required".into()))?;
        let rule_rx = self
            .rule_rx
            .ok_or_else(|| BotError::NotInitialized("rule_rx required".into()))?;
        let response_tx = self
            .response_tx
            .ok_or_else(|| BotError::NotInitialized("response_tx required".into()))?;

        // Load aliases from store
        let alias_pairs = store.list_aliases().await?;
        let aliases = alias_pairs.into_iter().collect();

        // Build router
        let mut router = Router::new(aliases, vec![]);

        // Build handler registry with builtins
        let mut handlers = HandlerRegistry::new();
        crate::handler::builtin::register_builtins(&mut handlers)?;

        // Register builtin command names in prefix router
        for cmd in crate::handler::builtin::BUILTIN_COMMANDS {
            router.register_command(cmd);
        }

        // Auto-register connector actions as commands
        {
            let reg = registry.read().await;
            crate::handler::connector::auto_register_connector_commands(
                &mut handlers,
                router.prefix_mut(),
                &reg,
            )?;
        }

        // AI adapter — defaults to NoopAdapter if not provided
        let ai_adapter: Arc<dyn springtale_ai::AiAdapter> = self
            .ai_adapter
            .unwrap_or_else(|| Arc::new(springtale_ai::NoopAdapter));

        // Derive or load memory encryption key from config store.
        // The key persists across restarts so encrypted messages remain readable.
        let encryption_key = match store.get_config("memory:encryption_key").await {
            Ok(Some(key_hex)) => {
                let bytes = hex::decode(key_hex.trim_matches('"'))
                    .map_err(|e| BotError::Memory(format!("invalid encryption key hex: {e}")))?;
                let key: [u8; 32] = bytes
                    .try_into()
                    .map_err(|_| BotError::Memory("encryption key must be 32 bytes".into()))?;
                key
            }
            _ => {
                // First run: generate a random key and persist it
                use rand::RngCore;
                let mut key = [0u8; 32];
                rand::thread_rng().fill_bytes(&mut key);
                let key_hex = hex::encode(key);
                if let Err(e) = store
                    .set_config("memory:encryption_key", &format!("\"{key_hex}\""))
                    .await
                {
                    tracing::warn!(error = %e, "failed to persist memory encryption key");
                }
                key
            }
        };

        let context = ConversationContext::new(
            store.clone(),
            self.config.context_window,
            ai_adapter.clone(),
            encryption_key,
        );

        // Create cadence bus for cooperation (§5.4)
        let (cadence, cadence_reports_rx) = CadenceBus::new(Duration::from_secs(1), 64);
        let cadence = Arc::new(cadence);
        let cadence_rx = cadence.subscribe();

        // Formation command receiver — provided by caller who passes the
        // sender to RuntimeState so operations can reach the bot event loop.
        let formation_cmd_rx = self
            .formation_cmd_rx
            .ok_or_else(|| BotError::NotInitialized("formation_cmd_rx required".into()))?;

        // Spawn cadence tick task (external clock, Necrodancer pattern)
        let cadence_run = cadence.clone();
        tokio::spawn(async move {
            cadence_run.run().await;
        });

        // Use pre-created formations handle if provided, otherwise create fresh.
        let formations = self
            .formations_handle
            .unwrap_or_else(|| Arc::new(RwLock::new(Vec::new())));

        // Gossip substrate: default to in-memory if the builder didn't
        // inject one. Single-process deployments never need more than
        // this; cross-process deployments inject a chitchat-backed store.
        let gossip_store: Arc<dyn springtale_cooperation::awareness::GossipStore> = self
            .gossip_store
            .unwrap_or_else(|| {
                Arc::new(
                    springtale_cooperation::awareness::InMemoryGossipStore::new(),
                )
            });

        // Role registry and capability bridge are required — the daemon
        // injects the shared instance from `RuntimeState` so there is
        // exactly one of each per process. Tests must construct their
        // own and pass them explicitly; there's no fallback path.
        let role_registry = self.role_registry.ok_or_else(|| {
            BotError::NotInitialized(
                "role_registry required (pass RuntimeState::role_registry)".into(),
            )
        })?;
        let capability_bridge = self.capability_bridge.ok_or_else(|| {
            BotError::NotInitialized(
                "capability_bridge required (pass RuntimeState::capability_bridge)".into(),
            )
        })?;

        Ok(Bot {
            router,
            handlers,
            store,
            registry,
            engine,
            config: self.config,
            context,
            ai_adapter,
            sentinel: self
                .sentinel
                .ok_or_else(|| BotError::NotInitialized("sentinel required".into()))?,
            connector_rx,
            rule_rx,
            response_tx,
            formations,
            cadence,
            cadence_rx,
            cadence_reports_rx,
            formation_cmd_rx,
            gossip_store,
            role_registry,
            capability_bridge,
            // L6 evaluator + executor are stateless rule tables / dispatcher
            // structs; both `Default::default()` is safe and matches the
            // rule thresholds in
            // `crates/springtale-bot/src/orchestrator/intervention/evaluator/thresholds.rs`.
            intervention_evaluator:
                crate::orchestrator::intervention::evaluator::RuleBasedEvaluator::default(),
            intervention_action:
                crate::orchestrator::intervention::action::DefaultInterventionAction,
            canvas_tx: self.canvas_tx,
            cooperation_tx: self.cooperation_tx,
        })
    }
}

impl Default for BotBuilder {
    fn default() -> Self {
        Self::new()
    }
}
