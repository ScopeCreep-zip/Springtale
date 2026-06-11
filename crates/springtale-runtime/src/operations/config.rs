//! Config operations — runtime configuration read/write.
//!
//! Stores config in SQLite via the config_store table (migration 006).
//! Enables UI-driven config changes without restart.
//!
//! For AI adapter changes, hot-swaps the adapter via ArcSwap.
//! For connector configs, stores for next connector reload.

use std::sync::Arc;

use serde_json::Value;
use springtale_store::StorageBackend;

use crate::error::OperationError;
use crate::state::RuntimeState;

/// Get a config value by key.
pub async fn get_config(store: &dyn StorageBackend, key: &str) -> Result<Value, OperationError> {
    let raw = store.get_config(key).await.map_err(OperationError::Store)?;

    match raw {
        Some(json_str) => serde_json::from_str(&json_str)
            .map_err(|e| OperationError::Validation(format!("invalid config JSON for {key}: {e}"))),
        None => Ok(Value::Null),
    }
}

/// Set a config value (upsert).
pub async fn set_config(
    store: &dyn StorageBackend,
    key: &str,
    value: Value,
) -> Result<(), OperationError> {
    let json_str = serde_json::to_string(&value)
        .map_err(|e| OperationError::Validation(format!("failed to serialize config: {e}")))?;

    store
        .set_config(key, &json_str)
        .await
        .map_err(OperationError::Store)?;

    tracing::info!(key = key, "config updated");
    Ok(())
}

/// List all config entries.
pub async fn list_config(
    store: &dyn StorageBackend,
) -> Result<Vec<(String, Value)>, OperationError> {
    let raw = store.list_config().await.map_err(OperationError::Store)?;

    let mut entries = Vec::new();
    for (key, json_str) in raw {
        let value: Value = serde_json::from_str(&json_str).unwrap_or(Value::Null);
        entries.push((key, value));
    }
    Ok(entries)
}

/// Build an `AiAdapter` from a config JSON `{ "type": "ollama"|"openai"|"anthropic"|"noop", ... }`.
///
/// **Single source of truth for adapter construction** — reused by every layer
/// of the AI command hierarchy (unit `ai:{agent_id}`, squad `ai:formation:{id}`,
/// colony `ai:colony`) and the global hot-swap. Async because the Ollama branch
/// verifies the model pin (OWASP LLM03 / LLM10 — training-data /
/// model-supply-chain poisoning; network-free when no digest is pinned).
pub async fn build_adapter(
    config: &Value,
) -> Result<Arc<dyn springtale_ai::AiAdapter>, OperationError> {
    let adapter_type = config
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("noop");
    let adapter: Arc<dyn springtale_ai::AiAdapter> = match adapter_type {
        "ollama" => {
            let cfg: springtale_ai::OllamaConfig = serde_json::from_value(config.clone())
                .map_err(|e| OperationError::Validation(format!("invalid ollama config: {e}")))?;
            springtale_ai::verify_model_pin(Some(&cfg))
                .await
                .map_err(|e| {
                    OperationError::Validation(format!("ollama model pin verification failed: {e}"))
                })?;
            springtale_ai::create_adapter(Some(&cfg), None, None).map_err(|e| {
                OperationError::Validation(format!("failed to create ollama adapter: {e}"))
            })?
        }
        "openai" => {
            let cfg: springtale_ai::OpenAiConfig = serde_json::from_value(config.clone())
                .map_err(|e| OperationError::Validation(format!("invalid openai config: {e}")))?;
            springtale_ai::create_adapter(None, Some(&cfg), None).map_err(|e| {
                OperationError::Validation(format!("failed to create openai adapter: {e}"))
            })?
        }
        "anthropic" => {
            let cfg: springtale_ai::AnthropicConfig = serde_json::from_value(config.clone())
                .map_err(|e| {
                    OperationError::Validation(format!("invalid anthropic config: {e}"))
                })?;
            springtale_ai::create_adapter(None, None, Some(&cfg)).map_err(|e| {
                OperationError::Validation(format!("failed to create anthropic adapter: {e}"))
            })?
        }
        _ => springtale_ai::create_adapter(None, None, None).map_err(|e| {
            OperationError::Validation(format!("failed to create noop adapter: {e}"))
        })?,
    };
    Ok(adapter)
}

/// Set the AI adapter config and hot-swap the runtime (global) adapter.
///
/// Config JSON must have a "type" field: "noop", "ollama", "openai", "anthropic".
/// On success, the new adapter is atomically swapped into RuntimeState.
pub async fn set_ai_adapter(state: &RuntimeState, config: Value) -> Result<(), OperationError> {
    let adapter_type = config
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("noop")
        .to_owned();

    // Persist to config store
    set_config(&*state.store, "ai_adapter", config.clone()).await?;

    // Build via the shared factory and hot-swap (atomic, lock-free).
    let new_adapter = build_adapter(&config).await?;
    state.ai_adapter.store(Arc::new(new_adapter));
    tracing::info!(adapter = adapter_type, "AI adapter hot-swapped");

    Ok(())
}

/// Store a connector config for future loading.
///
/// Does NOT instantiate the connector immediately — that requires
/// a registry reload. Stores the config so next init picks it up.
pub async fn set_connector_config(
    state: &RuntimeState,
    name: &str,
    config: Value,
) -> Result<(), OperationError> {
    let key = format!("connector:{name}");
    set_config(&*state.store, &key, config).await?;
    tracing::info!(connector = name, "connector config stored");
    Ok(())
}

/// Configure AI adapter — persists config under a target key and hot-swaps.
///
/// Supports multi-level AI config (RTS-style stance inheritance):
/// - `"ai:global"` — canvas-level default for all agents
/// - `"ai:formation:{id}"` — formation-level override
/// - `"ai:{agentId}"` — individual agent override
///
/// Only hot-swaps the global adapter when target is `"ai:global"`.
/// Per-agent/formation configs are resolved at dispatch time via `resolve_ai_config`.
pub async fn configure_ai_adapter(
    state: &RuntimeState,
    target: &str,
    config: Value,
) -> Result<(), OperationError> {
    set_config(&*state.store, target, config.clone()).await?;
    // Only hot-swap global adapter if target is canvas-level
    if target == "ai:global" {
        set_ai_adapter(state, config).await?;
    }
    Ok(())
}

/// Resolve AI config: agent → formation → canvas, first non-null wins.
///
/// RTS pattern: individual stance overrides group stance overrides global.
/// Each level stores config in the config store; `None`/null means "inherit."
pub async fn resolve_ai_config(
    store: &dyn springtale_store::StorageBackend,
    agent_id: &str,
    formation_id: Option<&str>,
) -> Result<Value, OperationError> {
    // Agent level
    let agent_config = get_config(store, &format!("ai:{agent_id}")).await?;
    if !agent_config.is_null() {
        return Ok(agent_config);
    }

    // Formation level
    if let Some(fid) = formation_id {
        let formation_config = get_config(store, &format!("ai:formation:{fid}")).await?;
        if !formation_config.is_null() {
            return Ok(formation_config);
        }
    }

    // Canvas (global) level
    get_config(store, "ai:global").await
}

/// Upsert connector config — setup if new, update config if already loaded.
///
/// Replaces the frontend branching pattern that checks if connector exists
/// before deciding between setup and config update.
pub async fn upsert_connector_config(
    state: &RuntimeState,
    name: &str,
    config: Value,
) -> Result<bool, OperationError> {
    // Check if connector is already loaded in registry
    let is_loaded = {
        let registry = state.registry.read().await;
        registry.get(name).is_some()
    };

    if is_loaded {
        set_connector_config(state, name, config).await?;
        Ok(false) // was update
    } else {
        // Setup = store config + load connector
        super::connectors::setup_connector(state, name, config).await?;
        Ok(true) // was new setup
    }
}

/// Toggle guard mode for a formation.
///
/// Replaces the frontend read-modify-write pattern on guard config.
pub async fn toggle_formation_guard(
    state: &RuntimeState,
    formation_id: &str,
) -> Result<bool, OperationError> {
    let key = format!("guard:{formation_id}");
    let current = get_config(&*state.store, &key).await?;
    let is_enabled = !current.is_null();
    if is_enabled {
        set_config(&*state.store, &key, Value::Null).await?;
    } else {
        set_config(&*state.store, &key, serde_json::json!({ "enabled": true })).await?;
    }
    Ok(!is_enabled) // returns new state
}
