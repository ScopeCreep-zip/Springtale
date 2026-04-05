//! Shared initialization — extracted from springtaled's boot sequence.
//!
//! These functions are the reusable core that both springtaled and
//! the desktop app call. No background tasks spawned here — that's
//! app-specific (daemon spawns scheduler/bot, desktop spawns Tauri).

use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::RwLock;

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
pub async fn init(config: &RuntimeConfig) -> Result<RuntimeState> {
    let store = init_store(&config.store).await?;
    tracing::info!("store initialized");

    let engine = init_engine(&store).await?;
    let registry = init_registry(&store, &config.connector_configs).await?;
    let ai_adapter = init_adapter(config)?;
    let sentinel = init_sentinel(config, &store);

    // Canvas/A2UI
    let canvas = Arc::new(tokio::sync::RwLock::new(
        springtale_core::canvas::CanvasState::default(),
    ));
    let (canvas_tx, _) = tokio::sync::broadcast::channel(64);

    Ok(RuntimeState {
        store,
        registry,
        engine,
        ai_adapter,
        sentinel,
        canvas,
        canvas_tx,
    })
}

/// Initialize the store backend.
async fn init_store(
    config: &crate::config::StoreConfig,
) -> Result<Arc<dyn springtale_store::StorageBackend>> {
    if config.ephemeral {
        tracing::warn!("EPHEMERAL MODE — all state in memory, lost on exit");
        Ok(Arc::new(springtale_store::backend::InMemoryBackend::new()))
    } else {
        tracing::info!(path = %config.path.display(), "opening SQLite store");
        if let Some(parent) = config.path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create data directory: {}", parent.display())
            })?;
        }
        Ok(Arc::new(
            SqliteBackend::open(&config.path).context("failed to open SQLite store")?,
        ))
    }
}

/// Load rules from store into a RuleEngine.
async fn init_engine(
    store: &Arc<dyn springtale_store::StorageBackend>,
) -> Result<Arc<RwLock<RuleEngine>>> {
    let rules = store
        .list_rules()
        .await
        .context("failed to load rules from store")?;

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
) -> Result<Arc<RwLock<ConnectorRegistry>>> {
    use springtale_connector::factory::FactoryEntry;

    let mut registry = ConnectorRegistry::new(CapabilityPolicy::Interactive);
    let mut registered = 0u32;

    for entry in inventory::iter::<FactoryEntry> {
        let factory = entry.factory;
        let key = factory.config_key();

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
        } else if !factory.requires_config() {
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

    tracing::info!(registered, "connector registry initialized");
    Ok(Arc::new(RwLock::new(registry)))
}

/// Create an AI adapter from config. Uses the factory from springtale-ai.
fn init_adapter(config: &RuntimeConfig) -> Result<Arc<dyn springtale_ai::AiAdapter>> {
    springtale_ai::create_adapter(
        config.ai_ollama.as_ref(),
        config.ai_openai.as_ref(),
        config.ai_anthropic.as_ref(),
    )
    .map_err(|e| anyhow::anyhow!("failed to create AI adapter: {e}"))
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
