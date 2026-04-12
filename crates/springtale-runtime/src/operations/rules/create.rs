use springtale_core::rule::types::{Rule, RuleId};

use super::{MAX_RULES, RuntimeState};
use crate::error::OperationError;

/// Create a new rule — adds to both store and engine.
///
/// If engine rejects the rule, rolls back the store insert.
/// Same logic as springtaled POST /rules.
pub async fn create_rule(state: &RuntimeState, rule: Rule) -> Result<RuleId, OperationError> {
    // Check count limit
    {
        let engine = state.engine.read().await;
        if engine.list_rules().len() >= MAX_RULES {
            return Err(OperationError::Validation(format!(
                "rule count limit reached ({MAX_RULES})"
            )));
        }
    }

    let rule_id = rule.id;

    // Add to store
    state.store.insert_rule(&rule).await?;

    // Add to engine — rollback store on failure
    {
        let mut engine = state.engine.write().await;
        if let Err(e) = engine.add_rule(rule) {
            tracing::error!(error = %e, "rule rejected by engine, rolling back store");
            let _ = state.store.delete_rule(&rule_id).await;
            return Err(OperationError::Rule(format!("invalid rule: {e}")));
        }
    }

    Ok(rule_id)
}

/// Delete a rule from both engine and store.
pub async fn delete_rule(state: &RuntimeState, id: &RuleId) -> Result<(), OperationError> {
    // Remove from engine first
    {
        let mut engine = state.engine.write().await;
        engine.remove_rule(id);
    }

    // Remove from store
    state.store.delete_rule(id).await?;

    Ok(())
}

/// Toggle a rule's enabled/disabled status in both store and engine.
pub async fn toggle_rule(
    state: &RuntimeState,
    id: &RuleId,
    enabled: bool,
) -> Result<(), OperationError> {
    // Update store
    state.store.toggle_rule(id, enabled).await?;

    // Reload rule from store and update engine
    let rules = state.store.list_rules().await?;
    if let Some(rule) = rules.into_iter().find(|r| &r.id == id) {
        let mut engine = state.engine.write().await;
        engine.remove_rule(id);
        let _ = engine.add_rule(rule);
    }

    Ok(())
}

/// Update a rule — replaces it in both store and engine.
///
/// Inserts the new rule first (safe — store is idempotent on ID),
/// then removes old from engine, adds new. Trigger scheduling
/// is app-specific (caller handles cron/watcher reschedule).
pub async fn update_rule(
    state: &RuntimeState,
    id: &RuleId,
    mut rule: Rule,
) -> Result<(), OperationError> {
    // Force rule ID to match (prevents ID mismatch)
    rule.id = *id;

    // Insert new rule first — if this fails, old rule is still intact
    state.store.insert_rule(&rule).await?;

    // Remove old from engine, add new
    {
        let mut engine = state.engine.write().await;
        engine.remove_rule(id);
        if let Err(e) = engine.add_rule(rule) {
            tracing::error!(error = %e, "updated rule rejected by engine");
            return Err(OperationError::Rule(format!("invalid rule: {e}")));
        }
    }

    Ok(())
}

/// Request to create a connector-event rule with simple fields.
///
/// Frontend sends field names, backend assembles the full Rule struct.
/// Eliminates hardcoded "ConnectorEvent"/"RunConnector" type tags in frontend.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateConnectorRuleRequest {
    pub name: String,
    pub trigger_connector: String,
    pub trigger_event: String,
    pub action_connector: String,
    pub action_name: String,
    #[serde(default)]
    pub conditions: Vec<springtale_core::rule::Condition>,
}

/// Create a connector-event rule from simple field names.
///
/// Replaces the frontend pattern of assembling `{ type: "ConnectorEvent", ... }`
/// and `{ type: "RunConnector", ... }` payloads.
pub async fn create_connector_rule(
    state: &RuntimeState,
    req: CreateConnectorRuleRequest,
) -> Result<RuleId, OperationError> {
    let rule = Rule {
        id: RuleId::new(),
        name: req.name,
        description: String::new(),
        status: springtale_core::rule::types::RuleStatus::Disabled,
        version: springtale_core::rule::types::RuleVersion(1),
        trigger: springtale_core::rule::Trigger::ConnectorEvent {
            connector: req.trigger_connector,
            event: req.trigger_event,
        },
        conditions: req.conditions,
        actions: if req.action_name.is_empty() {
            vec![]
        } else {
            vec![springtale_core::rule::Action::RunConnector {
                connector: req.action_connector,
                action: req.action_name,
                params: serde_json::Map::new(),
            }]
        },
    };

    create_rule(state, rule).await
}
