use springtale_core::rule::types::{Rule, RuleId};
use springtale_store::StorageBackend;

use super::{RuleSummary, RuntimeState};
use crate::error::OperationError;

/// List all rules from the engine (authoritative source).
pub async fn list_rules(state: &RuntimeState) -> Vec<RuleSummary> {
    let engine = state.engine.read().await;
    let activation_errors = state
        .store
        .get_rule_activation_errors()
        .await
        .unwrap_or_default();
    engine
        .list_rules()
        .iter()
        .map(|r| {
            let id_str = r.id.to_string();
            let activation_error = activation_errors.get(&id_str).cloned();
            RuleSummary {
                id: id_str,
                name: r.name.clone(),
                status: format!("{:?}", r.status).to_lowercase(),
                trigger_type: r.trigger.trigger_type().to_owned(),
                connector_name: r.trigger.connector_name(),
                activation_error,
            }
        })
        .collect()
}

/// List rules that belong to a specific connector.
///
/// Matches rules where the trigger connector matches the given name.
/// Replaces frontend `rules.filter(r => r.connector === id)` pattern.
pub async fn list_rules_for_connector(
    state: &RuntimeState,
    connector_name: &str,
) -> Vec<RuleSummary> {
    list_rules(state)
        .await
        .into_iter()
        .filter(|r| r.connector_name.as_deref() == Some(connector_name))
        .collect()
}

// ── Store-only operations (CLI) ──────────────────────────────────────────────

/// List rules from the persistent store.
///
/// Used by CLI which doesn't load the engine.
pub async fn list_rules_from_store(
    store: &dyn StorageBackend,
) -> Result<Vec<Rule>, OperationError> {
    store.list_rules().await.map_err(OperationError::Store)
}

/// Add a rule to the persistent store (no engine validation).
///
/// Used by CLI. The engine validates on next springtaled start.
pub async fn add_rule_to_store(
    store: &dyn StorageBackend,
    rule: &Rule,
) -> Result<RuleId, OperationError> {
    store.insert_rule(rule).await.map_err(OperationError::Store)
}

/// Toggle a rule in the persistent store only.
pub async fn toggle_rule_in_store(
    store: &dyn StorageBackend,
    id: &RuleId,
    enabled: bool,
) -> Result<(), OperationError> {
    store
        .toggle_rule(id, enabled)
        .await
        .map_err(OperationError::Store)
}

/// Delete a rule from the persistent store.
pub async fn delete_rule_from_store(
    store: &dyn StorageBackend,
    id: &RuleId,
) -> Result<(), OperationError> {
    store.delete_rule(id).await.map_err(OperationError::Store)
}
