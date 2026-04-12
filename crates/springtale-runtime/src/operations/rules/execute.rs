use serde::Serialize;

use springtale_core::rule::engine::TriggerEvent;
use springtale_core::rule::types::{Rule, RuleId};

use super::RuntimeState;
use crate::error::OperationError;

/// Run result from a dry-run rule evaluation.
#[derive(Debug, Serialize)]
pub struct RunResult {
    pub matched: bool,
    pub actions_count: usize,
}

/// Dry-run a rule — creates a synthetic trigger and evaluates it.
///
/// No side effects: actions are counted but not executed.
/// Uses the engine (runtime operation).
pub async fn run_rule(state: &RuntimeState, id: &RuleId) -> Result<RunResult, OperationError> {
    let engine = state.engine.read().await;

    let rule = engine
        .list_rules()
        .into_iter()
        .find(|r| &r.id == id)
        .ok_or_else(|| OperationError::NotFound(format!("rule {id}")))?;

    let event = build_synthetic_trigger(rule);
    let matches = engine.evaluate(&event);
    let actions_count = matches.iter().map(|m| m.actions.len()).sum::<usize>();

    Ok(RunResult {
        matched: !matches.is_empty(),
        actions_count,
    })
}

/// Dry-run a single rule without a RuntimeState.
///
/// Creates a temporary engine, loads the rule, and evaluates against a
/// synthetic trigger. Used by the CLI which doesn't load the full runtime.
pub fn run_rule_standalone(rule: &Rule) -> RunResult {
    let event = build_synthetic_trigger(rule);

    let mut engine = springtale_core::rule::engine::RuleEngine::new();
    let _ = engine.add_rule(rule.clone());
    let matches = engine.evaluate(&event);
    let actions_count = matches.iter().map(|m| m.actions.len()).sum::<usize>();

    RunResult {
        matched: !matches.is_empty(),
        actions_count,
    }
}

/// Build a synthetic trigger event that matches a rule's trigger definition.
///
/// Shared between `run_rule` (runtime) and `run_rule_standalone` (CLI).
pub fn build_synthetic_trigger(rule: &Rule) -> TriggerEvent {
    match &rule.trigger {
        springtale_core::rule::Trigger::Cron { .. } => TriggerEvent {
            trigger_type: "Cron".to_owned(),
            connector: None,
            event: None,
            payload: serde_json::json!({"manual_trigger": true}),
        },
        springtale_core::rule::Trigger::FileWatch { path, event: ev } => TriggerEvent {
            trigger_type: "FileWatch".to_owned(),
            connector: None,
            event: Some(format!("{path}:{ev}")),
            payload: serde_json::json!({"manual_trigger": true, "path": path}),
        },
        springtale_core::rule::Trigger::Webhook { path } => TriggerEvent {
            trigger_type: "Webhook".to_owned(),
            connector: None,
            event: Some(path.clone()),
            payload: serde_json::json!({"manual_trigger": true}),
        },
        springtale_core::rule::Trigger::ConnectorEvent {
            connector,
            event: ev,
        } => TriggerEvent {
            trigger_type: "ConnectorEvent".to_owned(),
            connector: Some(connector.clone()),
            event: Some(ev.clone()),
            payload: serde_json::json!({"manual_trigger": true}),
        },
        springtale_core::rule::Trigger::SystemEvent { event: ev } => TriggerEvent {
            trigger_type: "SystemEvent".to_owned(),
            connector: None,
            event: Some(ev.clone()),
            payload: serde_json::json!({"manual_trigger": true}),
        },
    }
}

/// Test result for a connector.
#[derive(Debug, Serialize)]
pub struct ConnectorTestResult {
    pub matched: bool,
    pub rule_name: Option<String>,
}

/// Test a connector by finding and dry-running its first rule.
///
/// Replaces the frontend two-step: find rule -> run rule.
pub async fn test_connector(
    state: &RuntimeState,
    connector_name: &str,
) -> Result<ConnectorTestResult, OperationError> {
    let engine = state.engine.read().await;

    let rule = engine
        .list_rules()
        .into_iter()
        .find(|r| match &r.trigger {
            springtale_core::rule::Trigger::ConnectorEvent { connector, .. } => {
                connector == connector_name
            }
            _ => false,
        })
        .ok_or_else(|| {
            OperationError::NotFound(format!("no rules for connector {connector_name}"))
        })?;

    let rule_name = rule.name.clone();
    let event = build_synthetic_trigger(rule);
    let matches = engine.evaluate(&event);

    Ok(ConnectorTestResult {
        matched: !matches.is_empty(),
        rule_name: Some(rule_name),
    })
}

/// Reassign a rule to a different connector.
///
/// Updates the trigger connector and (if RunConnector) the first action's
/// connector. Preserves conditions, name, and other fields.
pub async fn reassign_rule_connector(
    state: &RuntimeState,
    id: &RuleId,
    new_connector: &str,
) -> Result<(), OperationError> {
    let engine = state.engine.read().await;
    let rule = engine
        .list_rules()
        .into_iter()
        .find(|r| &r.id == id)
        .ok_or_else(|| OperationError::NotFound(format!("rule {id}")))?
        .clone();
    drop(engine);

    let new_trigger = match &rule.trigger {
        springtale_core::rule::Trigger::ConnectorEvent { event, .. } => {
            springtale_core::rule::Trigger::ConnectorEvent {
                connector: new_connector.to_owned(),
                event: event.clone(),
            }
        }
        other => other.clone(),
    };

    let new_actions: Vec<springtale_core::rule::Action> = rule
        .actions
        .iter()
        .map(|a| match a {
            springtale_core::rule::Action::RunConnector { action, params, .. } => {
                springtale_core::rule::Action::RunConnector {
                    connector: new_connector.to_owned(),
                    action: action.clone(),
                    params: params.clone(),
                }
            }
            other => other.clone(),
        })
        .collect();

    let updated = Rule {
        trigger: new_trigger,
        actions: new_actions,
        ..rule
    };

    super::update_rule(state, id, updated).await
}
