//! Shared initialization — extracted from springtaled's boot sequence.
//!
//! These functions are the reusable core that both springtaled and
//! the desktop app call. No background tasks spawned here — that's
//! app-specific (daemon spawns scheduler/bot, desktop spawns Tauri).

use std::sync::Arc;

use tokio::sync::RwLock;

use crate::error::OperationError;

use springtale_connector::capability::grant::CapabilityPolicy;
use springtale_connector::registry::store::ConnectorRegistry;
use springtale_core::rule::engine::RuleEngine;
use springtale_store::backend::sqlite::SqliteBackend;

use crate::config::RuntimeConfig;
use crate::state::RuntimeState;

/// Initialize the full shared runtime.
///
/// Equivalent to springtaled's boot Steps 1-5b:
/// store → rules → connectors → AI adapter → sentinel.
///
/// Vault is NOT initialized here — desktop handles it via UI
/// (user types passphrase), springtaled reads from env/file.
pub async fn init(
    config: &RuntimeConfig,
    formation_cmd_tx: tokio::sync::mpsc::Sender<springtale_cooperation::command::FormationCommand>,
    live_formations: Option<Arc<dyn crate::state::LiveFormationReader>>,
) -> Result<RuntimeState, OperationError> {
    let store = init_store(&config.store).await?;
    tracing::info!("store initialized");

    let engine = init_engine(&store).await?;

    // Shared WASM engine — all WASM connectors use the same engine
    // so epoch interrupts work from a single ticker.
    let wasm_engine = Arc::new(
        springtale_connector::wasm::WasmEngine::new(
            springtale_connector::wasm::SandboxLimits::default(),
        )
        .map_err(|e| OperationError::Init(format!("WASM engine creation failed: {e}")))?,
    );

    // Shared per-tier `InstancePre` cache (§16). Every WASM connector's
    // module is pre-instantiated against all four tiers here so momentum
    // transitions are hash-lookup + `InstancePre::instantiate` with no
    // Linker rebuild. Also lives in `RuntimeState` so the forthcoming
    // `CapabilityBridge` can route tier transitions to specific hosts.
    let wasm_tier_cache = Arc::new(
        springtale_connector::wasm::WasmTierCache::new(wasm_engine.clone())
            .map_err(|e| OperationError::Init(format!("WASM tier cache init failed: {e}")))?,
    );

    let registry = init_registry(
        &store,
        &config.connector_configs,
        &wasm_engine,
        &wasm_tier_cache,
    )
    .await?;
    let ai_adapter_arc = init_adapter(config)?;
    let sentinel = init_sentinel(config, &store);

    // Start WASM epoch ticker — increments every 1s so wall-clock
    // timeouts actually fire. Without this, a malicious WASM module
    // doing blocking I/O could run forever (fuel only counts instructions).
    {
        let ticker_engine = wasm_engine.engine().clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
            loop {
                interval.tick().await;
                ticker_engine.increment_epoch();
            }
        });
        tracing::info!("WASM epoch ticker started (1s interval)");
    }

    // Cooperation deposit sweeper — per COOPERATION.md §20.3: every 5s
    // delete `coop_deposits` rows whose `expires_at` is past. Without
    // this, abandoned environment-mediated handoffs accumulate
    // indefinitely because exactly-once collection removes claimed
    // rows but never expired ones.
    {
        let store_sweeper = store.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
            loop {
                interval.tick().await;
                match store_sweeper.coop_sweep_expired().await {
                    Ok(n) if n > 0 => {
                        tracing::debug!(swept = n, "cooperation deposit sweeper");
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, "deposit sweep failed");
                    }
                }
            }
        });
        tracing::info!("cooperation deposit sweeper started (5s interval)");
    }

    // Gossip substrate (§8). Selected by `CooperationConfig::cross_process`:
    // single-process deployments get `InMemoryGossipStore` (DashMap);
    // cross-process deployments spawn `ChitchatGossipStore` over UDP.
    let gossip_store: Arc<dyn springtale_cooperation::awareness::GossipStore> =
        init_gossip_store(&config.cooperation).await?;

    // SWIM liveness (§8.3). Only spawned when `cross_process = true` —
    // single-process deployments have no peer processes to probe. The
    // node also drives peer-scoped gossip sweeping: when SWIM declares
    // a peer dead, `MemberDown` fires and `GossipStore::remove_by_peer`
    // drops every entry the peer published, so `snapshots()` no longer
    // surfaces stale data. The identifier the sweep uses is the peer's
    // SocketAddr string — the same id chitchat stamps onto incoming
    // entries via `GossipEntry::with_peer_id`.
    let swim_node =
        init_swim_node(&config.cooperation, gossip_store.clone()).await?;

    // Canvas/A2UI
    let canvas = Arc::new(tokio::sync::RwLock::new(
        springtale_core::canvas::CanvasState::default(),
    ));
    let (canvas_tx, _) = tokio::sync::broadcast::channel(64);
    // Phase H2 — cooperation events bus. Capacity 512 covers ~30s of
    // headroom at 4 formations × 30Hz × ~5 events/tick. Lagged readers
    // drop silently per the events_stream.rs precedent.
    let (cooperation_tx, _) = tokio::sync::broadcast::channel(512);

    // Capability bridge (Phase 17) — binds the registry to a per-invocation
    // tier dispatch path. See `crate::cooperation::capability_bridge`.
    let capability_bridge = crate::cooperation::CapabilityBridge::new(registry.clone());

    // Role registry (Phase 21) — starts with built-in General/Information/
    // Support. Community roles are folded in by `register_manifest_roles`
    // during connector install, and by the same helper applied to any
    // manifests that were loaded from the store in `init_registry`.
    let role_registry = Arc::new(springtale_cooperation::role::RoleRegistry::with_builtins());
    register_persisted_manifest_roles(&store, &role_registry).await;

    Ok(RuntimeState {
        store,
        registry,
        engine,
        ai_adapter: Arc::new(arc_swap::ArcSwap::from(Arc::new(ai_adapter_arc))),
        sentinel,
        wasm_engine,
        wasm_tier_cache,
        capability_bridge,
        role_registry,
        canvas,
        canvas_tx,
        cooperation_tx,
        formation_cmd_tx,
        live_formations,
        gossip_store,
        swim_node,
    })
}

/// Walk all connector manifests already in the store and fold their
/// `roles` declarations into the shared `RoleRegistry`. Called once at
/// init. Future connector installs register directly via
/// `operations::connectors::install` (Phase 21 wiring).
async fn register_persisted_manifest_roles(
    store: &Arc<dyn springtale_store::StorageBackend>,
    role_registry: &Arc<springtale_cooperation::role::RoleRegistry>,
) {
    let Ok(rows) = store.list_connectors().await else {
        return;
    };
    for row in rows {
        let Ok(manifest) =
            serde_json::from_str::<springtale_connector::ConnectorManifest>(&row.manifest_json)
        else {
            tracing::warn!(
                connector = %row.name,
                "manifest JSON invalid — skipping role registration"
            );
            continue;
        };
        crate::cooperation::register_manifest_roles(role_registry, &manifest);
    }
}

/// Construct the SWIM liveness node when cross-process mode is on.
///
/// Also wires an event bridge from the node's broadcast channel into
/// the shared gossip store: `MemberDown` → `GossipStore::remove`. That
/// way awareness snapshots (`GossipStore::snapshots`) stop surfacing
/// stale entries for peers the SWIM protocol has given up on.
async fn init_swim_node(
    cfg: &crate::config::CooperationConfig,
    gossip_store: Arc<dyn springtale_cooperation::awareness::GossipStore>,
) -> Result<
    Option<Arc<springtale_cooperation::awareness::SwimNode>>,
    OperationError,
> {
    use std::num::NonZeroU32;
    use springtale_cooperation::awareness::{SwimNode, SwimNodeConfig};

    if !cfg.cross_process {
        return Ok(None);
    }

    let listen: std::net::SocketAddr = cfg
        .swim_listen_addr
        .as_deref()
        .unwrap_or("127.0.0.1:0")
        .parse()
        .map_err(|e| OperationError::Init(format!("invalid swim_listen_addr: {e}")))?;

    let seeds: Vec<std::net::SocketAddr> = cfg
        .swim_seeds
        .iter()
        .map(|s| {
            s.parse().map_err(|e| {
                OperationError::Init(format!("invalid swim seed {s}: {e}"))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let swim_cfg = SwimNodeConfig {
        listen,
        seeds,
        cluster_size: NonZeroU32::new(10).unwrap_or(NonZeroU32::MIN),
    };
    let node = SwimNode::spawn(swim_cfg)
        .await
        .map_err(|e| OperationError::Init(format!("swim spawn: {e}")))?;
    tracing::info!(
        addr = %node.local_addr(),
        seeds = cfg.swim_seeds.len(),
        "SWIM liveness node started"
    );

    // Spawn a dual-purpose consumer: (a) audit-log every SwimEvent so
    // peer liveness transitions are observable, and (b) on MemberDown
    // sweep the shared gossip store of any peer-owned snapshots so
    // awareness reads stop seeing defunct peers. This is the live
    // consumer that gives the SWIM node a real job beyond logs.
    //
    // Sweep strategy: entries received from remote peers carry
    // `GossipEntry::peer_id = Some(peer.to_string())` (stamped by the
    // chitchat adapter when decoding). `GossipStore::remove_by_peer`
    // filters on that field, so a peer going down cleanly drops every
    // entry the peer owned. Locally-published entries have
    // `peer_id = None` and are never swept by this path.
    {
        use springtale_cooperation::awareness::{SwimEvent, SwimSelfState};
        let mut rx = node.subscribe();
        let sweep_gossip = gossip_store.clone();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(SwimEvent::MemberUp(peer)) => {
                        tracing::info!(peer = %peer, "SWIM: peer up");
                    }
                    Ok(SwimEvent::MemberDown(peer)) => {
                        let peer_id = peer.to_string();
                        let removed = sweep_gossip.remove_by_peer(&peer_id).await;
                        tracing::warn!(
                            peer = %peer,
                            gossip_entries_swept = removed,
                            "SWIM: peer down"
                        );
                    }
                    Ok(SwimEvent::MemberRejoined(peer)) => {
                        tracing::info!(peer = %peer, "SWIM: peer rejoined");
                    }
                    Ok(SwimEvent::SelfState(SwimSelfState::Defunct)) => {
                        tracing::error!("SWIM: self declared defunct");
                    }
                    Ok(SwimEvent::SelfState(state)) => {
                        tracing::debug!(?state, "SWIM: self state");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "SWIM event consumer lagged");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    Ok(Some(Arc::new(node)))
}

/// Construct the gossip substrate based on cooperation config.
async fn init_gossip_store(
    cfg: &crate::config::CooperationConfig,
) -> Result<
    Arc<dyn springtale_cooperation::awareness::GossipStore>,
    OperationError,
> {
    use springtale_cooperation::awareness::{
        ChitchatGossipConfig, ChitchatGossipStore, InMemoryGossipStore,
    };

    if !cfg.cross_process {
        tracing::info!("gossip: in-memory (single-process)");
        return Ok(Arc::new(InMemoryGossipStore::new()));
    }

    let listen_str = cfg.chitchat_listen_addr.as_deref().ok_or_else(|| {
        OperationError::Init(
            "cooperation.cross_process = true requires chitchat_listen_addr".into(),
        )
    })?;
    let listen: std::net::SocketAddr = listen_str
        .parse()
        .map_err(|e| OperationError::Init(format!("invalid chitchat_listen_addr: {e}")))?;

    let chitchat_cfg = ChitchatGossipConfig {
        node_id: format!("springtale-{}", uuid::Uuid::new_v4()),
        listen_addr: listen,
        public_addr: listen,
        seeds: cfg.chitchat_seeds.clone(),
        cluster_id: cfg.cluster_id.clone(),
        gossip_interval: std::time::Duration::from_secs(1),
    };
    tracing::info!(
        addr = %listen,
        seeds = cfg.chitchat_seeds.len(),
        cluster = %cfg.cluster_id,
        "gossip: chitchat (cross-process)"
    );
    let store = ChitchatGossipStore::spawn(chitchat_cfg)
        .await
        .map_err(|e| OperationError::Init(format!("chitchat spawn: {e}")))?;
    Ok(Arc::new(store))
}

/// Initialize the store backend.
async fn init_store(
    config: &crate::config::StoreConfig,
) -> Result<Arc<dyn springtale_store::StorageBackend>, OperationError> {
    if config.ephemeral {
        tracing::warn!("EPHEMERAL MODE — all state in memory, lost on exit");
        Ok(Arc::new(springtale_store::backend::InMemoryBackend::new()))
    } else {
        tracing::info!(path = %config.path.display(), "opening SQLite store");
        if let Some(parent) = config.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                OperationError::Init(format!(
                    "failed to create data directory {}: {e}",
                    parent.display()
                ))
            })?;
        }
        let backend = if let Some(ref key) = config.encryption_key_hex {
            SqliteBackend::open_encrypted(&config.path, key)
                .map_err(|e| OperationError::Init(format!("failed to open encrypted store: {e}")))?
        } else {
            SqliteBackend::open(&config.path)
                .map_err(|e| OperationError::Init(format!("failed to open SQLite store: {e}")))?
        };
        Ok(Arc::new(backend))
    }
}

/// Load rules from store into a RuleEngine.
async fn init_engine(
    store: &Arc<dyn springtale_store::StorageBackend>,
) -> Result<Arc<RwLock<RuleEngine>>, OperationError> {
    let rules = store
        .list_rules()
        .await
        .map_err(|e| OperationError::Init(format!("failed to load rules: {e}")))?;

    let mut engine = RuleEngine::new();
    let mut loaded = 0;
    for rule in &rules {
        if let Err(e) = engine.add_rule(rule.clone()) {
            tracing::warn!(rule = %rule.name, error = %e, "skipping invalid rule");
        } else {
            loaded += 1;
        }
    }
    tracing::info!(total = rules.len(), loaded, "rule engine loaded");

    Ok(Arc::new(RwLock::new(engine)))
}

/// Discover compiled-in connectors via `inventory` and instantiate those
/// whose config sections are present.
async fn init_registry(
    _store: &Arc<dyn springtale_store::StorageBackend>,
    connector_configs: &std::collections::HashMap<String, serde_json::Value>,
    shared_wasm_engine: &Arc<springtale_connector::wasm::WasmEngine>,
    shared_wasm_tier_cache: &Arc<springtale_connector::wasm::WasmTierCache>,
) -> Result<Arc<RwLock<ConnectorRegistry>>, OperationError> {
    use springtale_connector::factory::FactoryEntry;

    let mut registry = ConnectorRegistry::new(CapabilityPolicy::Interactive);
    let mut registered = 0u32;

    // Load the set of connectors explicitly removed by the user.
    // Prevents auto-loading of no-config connectors (shell, filesystem)
    // that were removed via the UI.
    let removed_connectors: std::collections::HashSet<String> = _store
        .list_config()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(k, _)| k.strip_prefix("connector-removed:").map(|s| s.to_owned()))
        .collect();

    // First run detection: if onboarding hasn't completed yet, don't
    // auto-load no-config connectors (shell, filesystem). A fresh vault
    // should land on a blank canvas so the OOBE flow can guide the user.
    // After onboarding (or if the user explicitly adds connectors), these
    // will load normally on subsequent boots.
    let onboarded = _store
        .get_config("onboarded")
        .await
        .ok()
        .flatten()
        .map(|v| v.trim_matches('"') == "true")
        .unwrap_or(false);

    for entry in inventory::iter::<FactoryEntry> {
        let factory = entry.factory;
        let key = factory.config_key();

        // Skip connectors explicitly removed by the user
        if removed_connectors.contains(factory.name()) {
            tracing::debug!(
                connector = factory.name(),
                "skipping — explicitly removed by user"
            );
            continue;
        }

        if let Some(config_value) = connector_configs.get(key) {
            match factory.create(config_value.clone()).await {
                Ok(connector) => match registry.install_native(connector) {
                    Ok(name) => {
                        tracing::info!(connector = %name, "auto-registered connector");
                        registered += 1;
                    }
                    Err(e) => {
                        tracing::warn!(
                            connector = factory.name(),
                            error = %e,
                            "failed to install connector"
                        );
                    }
                },
                Err(e) => {
                    tracing::warn!(
                        connector = factory.name(),
                        error = %e,
                        "failed to instantiate connector, skipping"
                    );
                }
            }
        } else if !factory.requires_config() && onboarded {
            match factory
                .create(serde_json::Value::Object(Default::default()))
                .await
            {
                Ok(connector) => match registry.install_native(connector) {
                    Ok(name) => {
                        tracing::info!(connector = %name, "auto-registered (no config needed)");
                        registered += 1;
                    }
                    Err(e) => {
                        tracing::warn!(
                            connector = factory.name(),
                            error = %e,
                            "failed to install connector"
                        );
                    }
                },
                Err(e) => {
                    tracing::warn!(
                        connector = factory.name(),
                        error = %e,
                        "failed to instantiate default connector, skipping"
                    );
                }
            }
        } else {
            tracing::debug!(
                connector = factory.name(),
                config_key = key,
                "no config found, not loading"
            );
        }
    }

    // Also load connectors configured via UI (stored in config_store as "connector:{key}").
    // TOML configs take precedence — config store only loads connectors not already loaded.
    // This is the counterpart to setup_connector() which writes to config_store.
    let loaded_keys: Vec<String> = inventory::iter::<FactoryEntry>
        .into_iter()
        .filter(|e| connector_configs.contains_key(e.factory.config_key()))
        .map(|e| e.factory.config_key().to_owned())
        .collect();

    if let Ok(stored) = _store.list_config().await {
        for (key, value_json) in &stored {
            let Some(config_key) = key.strip_prefix("connector:") else {
                continue;
            };
            if loaded_keys.contains(&config_key.to_owned()) {
                continue; // already loaded from TOML
            }
            let Ok(config_value) = serde_json::from_str::<serde_json::Value>(value_json) else {
                continue;
            };
            for entry in inventory::iter::<FactoryEntry> {
                if entry.factory.config_key() == config_key {
                    match entry.factory.create(config_value.clone()).await {
                        Ok(connector) => match registry.install_native(connector) {
                            Ok(name) => {
                                tracing::info!(connector = %name, "loaded from config store");
                                registered += 1;
                            }
                            Err(e) => tracing::warn!(
                                connector = entry.factory.name(),
                                error = %e,
                                "failed to install connector from config store"
                            ),
                        },
                        Err(e) => tracing::warn!(
                            connector = entry.factory.name(),
                            error = %e,
                            "failed to create connector from config store"
                        ),
                    }
                    break;
                }
            }
        }
    }

    // Load persisted WASM connectors from store (installed via UI/CLI).
    // These are community connectors that were installed as .wasm packages
    // and persisted in the wasm_binaries table.
    {
        use springtale_connector::wasm::SandboxLimits;

        let wasm_binaries = _store.list_wasm_binaries().await.unwrap_or_default();
        if !wasm_binaries.is_empty() {
            for bin in wasm_binaries {
                if removed_connectors.contains(&bin.name) {
                    tracing::debug!(connector = %bin.name, "skipping removed WASM connector");
                    continue;
                }
                let manifest: springtale_connector::ConnectorManifest = match serde_json::from_str(
                    &bin.manifest_json,
                ) {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::warn!(connector = %bin.name, error = %e, "invalid WASM manifest JSON");
                        continue;
                    }
                };
                match registry.install_wasm(
                    shared_wasm_engine.clone(),
                    &bin.wasm_bytes,
                    manifest,
                    SandboxLimits::default(),
                    shared_wasm_tier_cache.clone(),
                ) {
                    Ok(name) => {
                        tracing::info!(connector = %name, "loaded WASM connector from store");
                        registered += 1;
                    }
                    Err(e) => {
                        tracing::warn!(connector = %bin.name, error = %e, "failed to load WASM connector");
                    }
                }
            }
        }
    }

    tracing::info!(registered, "connector registry initialized");
    Ok(Arc::new(RwLock::new(registry)))
}

/// Create an AI adapter from config. Uses the factory from springtale-ai.
fn init_adapter(
    config: &RuntimeConfig,
) -> Result<Arc<dyn springtale_ai::AiAdapter>, OperationError> {
    springtale_ai::create_adapter(
        config.ai_ollama.as_ref(),
        config.ai_openai.as_ref(),
        config.ai_anthropic.as_ref(),
    )
    .map_err(|e| OperationError::Init(format!("failed to create AI adapter: {e}")))
}

/// Initialize the sentinel behavioral monitor.
fn init_sentinel(
    config: &RuntimeConfig,
    store: &Arc<dyn springtale_store::StorageBackend>,
) -> Arc<springtale_sentinel::Sentinel> {
    let sentinel_config = config.sentinel.clone().unwrap_or_default();
    Arc::new(springtale_sentinel::Sentinel::new(
        sentinel_config,
        store.clone(),
    ))
}
