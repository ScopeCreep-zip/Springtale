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

    let registry = init_registry(&store, &config.connector_configs, &wasm_engine).await?;
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

    // Canvas/A2UI
    let canvas = Arc::new(tokio::sync::RwLock::new(
        springtale_core::canvas::CanvasState::default(),
    ));
    let (canvas_tx, _) = tokio::sync::broadcast::channel(64);

    Ok(RuntimeState {
        store,
        registry,
        engine,
        ai_adapter: Arc::new(arc_swap::ArcSwap::from(Arc::new(ai_adapter_arc))),
        sentinel,
        wasm_engine,
        canvas,
        canvas_tx,
        formation_cmd_tx,
        live_formations,
    })
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
