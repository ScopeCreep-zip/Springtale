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

/// Colony-level AI config key — the default every agent inherits.
pub const AI_COLONY_KEY: &str = "ai:colony";

/// One level of the AI command hierarchy. One config key per level:
/// `ai:colony`, `ai:formation:{formation_id}`, `ai:agent:{rule_id}`.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    specta::Type,
    utoipa::ToSchema,
)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum AiTarget {
    Colony,
    Formation { id: String },
    Agent { rule_id: uuid::Uuid },
}

impl AiTarget {
    /// The config-store key for this level.
    pub fn key(&self) -> String {
        match self {
            Self::Colony => AI_COLONY_KEY.to_owned(),
            Self::Formation { id } => format!("ai:formation:{id}"),
            Self::Agent { rule_id } => format!("ai:agent:{rule_id}"),
        }
    }
}

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

/// Configure the AI adapter at one level of the hierarchy.
///
/// Builds the adapter first so an invalid config is never persisted,
/// stores it under [`AiTarget::key`], hot-swaps the colony adapter when
/// the target is [`AiTarget::Colony`], and clears the bridge's built-adapter
/// cache so the next dispatch re-resolves from the store.
pub async fn configure_ai_adapter(
    state: &RuntimeState,
    target: AiTarget,
    config: Value,
) -> Result<(), OperationError> {
    let adapter = build_adapter(&config).await?;
    set_config(&*state.store, &target.key(), config).await?;
    if matches!(target, AiTarget::Colony) {
        state.ai_adapter.store(Arc::new(adapter));
        tracing::info!("colony AI adapter hot-swapped");
    }
    state.capability_bridge.invalidate_ai_cache().await;
    Ok(())
}

/// Resolve the AI config for a firing rule: rule → formation → colony,
/// first non-null wins. `Null` when no level is configured.
pub async fn resolve_ai_config(
    store: &dyn StorageBackend,
    rule_id: &uuid::Uuid,
    formation_id: Option<&str>,
) -> Result<Value, OperationError> {
    let keys = [
        Some(format!("ai:agent:{rule_id}")),
        formation_id.map(|f| format!("ai:formation:{f}")),
        Some(AI_COLONY_KEY.to_owned()),
    ];
    for key in keys.into_iter().flatten() {
        let v = get_config(store, &key).await?;
        if !v.is_null() {
            return Ok(v);
        }
    }
    Ok(Value::Null)
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

/// Config key holding a formation's guard-mode flag. The single source of
/// truth for the durable copy — every reader goes through
/// [`formation_guard_engaged`] and the only writer is
/// [`toggle_formation_guard`].
fn guard_key(formation_id: &str) -> String {
    format!("guard:{formation_id}")
}

/// Whether guard mode is engaged for a formation, read from the durable
/// config row. Deploy copies this into the live formation's
/// `constraints.guard_mode`, and [`toggle_formation_guard`] keeps the live
/// copy in step afterward, so the two agree.
pub async fn formation_guard_engaged(
    store: &dyn springtale_store::backend::StorageBackend,
    formation_id: &str,
) -> bool {
    !get_config(store, &guard_key(formation_id))
        .await
        .unwrap_or(Value::Null)
        .is_null()
}

/// Toggle guard mode for a formation.
///
/// Replaces the frontend read-modify-write pattern on guard config.
///
/// Writes the durable config row AND posts `FormationCommand::SetGuard` so the
/// live `Formation` in the bot tick loop picks the change up on its next
/// command drain. Without the command the live `constraints.guard_mode` would
/// keep whatever value deploy gave it, and engaging guard would protect
/// nothing until the formation was redeployed.
pub async fn toggle_formation_guard(
    state: &RuntimeState,
    formation_id: &str,
) -> Result<bool, OperationError> {
    let key = guard_key(formation_id);
    let is_enabled = formation_guard_engaged(&*state.store, formation_id).await;
    if is_enabled {
        set_config(&*state.store, &key, Value::Null).await?;
    } else {
        set_config(&*state.store, &key, serde_json::json!({ "enabled": true })).await?;
    }
    let engaged = !is_enabled;
    if let Ok(fid) = springtale_cooperation::types::FormationId::parse(formation_id) {
        let _ = state
            .formation_cmd_tx
            .send(
                springtale_cooperation::command::FormationCommand::SetGuard {
                    formation_id: fid,
                    engaged,
                },
            )
            .await;
    } else {
        tracing::warn!(
            formation = %formation_id,
            "guard toggled on an unparseable formation id — live formation not updated"
        );
    }
    Ok(engaged) // returns new state
}

#[cfg(test)]
mod tests {
    use super::*;
    use springtale_store::backend::sqlite::SqliteBackend;

    #[tokio::test]
    async fn test_resolve_ai_config_precedence_agent_over_formation_over_colony() {
        let store = SqliteBackend::open_in_memory().unwrap();
        let rule = uuid::Uuid::new_v4();
        let agent_key = AiTarget::Agent { rule_id: rule }.key();
        let formation_key = AiTarget::Formation { id: "f1".into() }.key();
        let colony = serde_json::json!({ "type": "noop", "level": "colony" });
        let formation = serde_json::json!({ "type": "noop", "level": "formation" });
        let agent = serde_json::json!({ "type": "noop", "level": "agent" });

        let resolve = |fid: Option<&'static str>| resolve_ai_config(&store, &rule, fid);

        assert_eq!(resolve(Some("f1")).await.unwrap(), Value::Null);

        set_config(&store, AI_COLONY_KEY, colony.clone())
            .await
            .unwrap();
        assert_eq!(resolve(Some("f1")).await.unwrap(), colony);

        set_config(&store, &formation_key, formation.clone())
            .await
            .unwrap();
        assert_eq!(resolve(Some("f1")).await.unwrap(), formation);
        // No formation in the firing context ⇒ the formation row is skipped.
        assert_eq!(resolve(None).await.unwrap(), colony);

        set_config(&store, &agent_key, agent.clone()).await.unwrap();
        assert_eq!(resolve(Some("f1")).await.unwrap(), agent);
    }
}
