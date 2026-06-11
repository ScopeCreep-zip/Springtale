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
///
/// Intentionally has no `specta::Type` derive: this struct transitively
/// references the recursive `Condition` enum (in `conditions`), and per
/// the rule-module specta policy (see `springtale_core::rule::types`
/// module doc), rule-shaped types stay schemars-only. The Tauri
/// command takes `serde_json::Value` and deserializes into this struct
/// internally — the frontend learns the shape from `get_rule_schema()`.
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
    /// W6 chain composer — additional action steps run in order after the
    /// primary action. When non-empty, the rule's actions become a single
    /// `Action::Chain` of `[primary, ...extra]`.
    #[serde(default)]
    pub extra_actions: Vec<ConnectorActionStep>,
    /// W6 all-of / any-of toggle. `false` (default) = every condition must
    /// hold (the engine's implicit AND over the flat list). `true` = any one
    /// suffices: the conditions are wrapped in a single `Condition::Or`.
    #[serde(default)]
    pub match_any: bool,
}

/// One step in a W6 action chain.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorActionStep {
    pub action_connector: String,
    pub action_name: String,
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
        conditions: build_conditions(req.conditions, req.match_any),
        actions: build_actions(req.action_connector, req.action_name, req.extra_actions),
        // Connector-event rules created from the UI form path are
        // global by default. Per-agent / per-formation scoping lands
        // when the rule-builder UI surfaces an owner picker (Phase A+).
        owner: springtale_core::rule::types::RuleOwner::Global,
    };

    create_rule(state, rule).await
}

/// W6: wrap the user's leaf conditions for all-of (default) vs any-of.
/// All-of leaves the flat list (the engine ANDs siblings implicitly).
/// Any-of with 2+ leaves wraps them in one `Condition::Or`; a single leaf
/// needs no wrapper either way.
fn build_conditions(
    conditions: Vec<springtale_core::rule::Condition>,
    match_any: bool,
) -> Vec<springtale_core::rule::Condition> {
    if match_any && conditions.len() > 1 {
        vec![springtale_core::rule::Condition::Or { conditions }]
    } else {
        conditions
    }
}

/// W6: build the rule's action list. One action → a bare `RunConnector`.
/// Two or more → a single `Action::Chain` of `[primary, ...extra]` so the
/// steps run in order. An empty primary action → no actions (monitor rule).
fn build_actions(
    action_connector: String,
    action_name: String,
    extra_actions: Vec<ConnectorActionStep>,
) -> Vec<springtale_core::rule::Action> {
    if action_name.is_empty() {
        return vec![];
    }
    let primary = springtale_core::rule::Action::RunConnector {
        connector: action_connector,
        action: action_name,
        params: serde_json::Map::new(),
    };
    if extra_actions.is_empty() {
        return vec![primary];
    }
    let mut steps = Vec::with_capacity(1 + extra_actions.len());
    steps.push(primary);
    for step in extra_actions {
        steps.push(springtale_core::rule::Action::RunConnector {
            connector: step.action_connector,
            action: step.action_name,
            params: serde_json::Map::new(),
        });
    }
    vec![springtale_core::rule::Action::Chain { steps }]
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use springtale_core::rule::{Action, Condition};

    fn leaf(field: &str) -> Condition {
        Condition::FieldEquals {
            field: field.to_owned(),
            value: serde_json::json!("x"),
        }
    }

    #[test]
    fn build_actions_single_is_bare_run_connector() {
        let actions = build_actions("c".into(), "a".into(), vec![]);
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::RunConnector { .. }));
    }

    #[test]
    fn build_actions_empty_name_yields_no_actions() {
        assert!(build_actions("c".into(), String::new(), vec![]).is_empty());
    }

    #[test]
    fn build_actions_multi_wraps_in_ordered_chain() {
        let extra = vec![
            ConnectorActionStep {
                action_connector: "c2".into(),
                action_name: "a2".into(),
            },
            ConnectorActionStep {
                action_connector: "c3".into(),
                action_name: "a3".into(),
            },
        ];
        let actions = build_actions("c1".into(), "a1".into(), extra);
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::Chain { steps } => {
                assert_eq!(steps.len(), 3);
                // Order preserved: primary first, then extras.
                match &steps[0] {
                    Action::RunConnector { action, .. } => assert_eq!(action, "a1"),
                    other => panic!("expected RunConnector, got {other:?}"),
                }
                match &steps[2] {
                    Action::RunConnector { action, .. } => assert_eq!(action, "a3"),
                    other => panic!("expected RunConnector, got {other:?}"),
                }
            }
            other => panic!("expected Chain, got {other:?}"),
        }
    }

    #[test]
    fn build_conditions_all_of_stays_flat() {
        let conds = build_conditions(vec![leaf("a"), leaf("b")], false);
        assert_eq!(conds.len(), 2);
        assert!(matches!(conds[0], Condition::FieldEquals { .. }));
    }

    #[test]
    fn build_conditions_any_of_wraps_in_or() {
        let conds = build_conditions(vec![leaf("a"), leaf("b")], true);
        assert_eq!(conds.len(), 1);
        match &conds[0] {
            Condition::Or { conditions } => assert_eq!(conditions.len(), 2),
            other => panic!("expected Or, got {other:?}"),
        }
    }

    #[test]
    fn build_conditions_single_any_of_needs_no_wrapper() {
        let conds = build_conditions(vec![leaf("a")], true);
        assert_eq!(conds.len(), 1);
        assert!(matches!(conds[0], Condition::FieldEquals { .. }));
    }
}
