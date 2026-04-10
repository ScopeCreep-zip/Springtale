//! Rule operations — create, delete, toggle, list, run, update.
//!
//! Runtime operations take `&RuntimeState` (need engine).
//! Store operations take `&dyn StorageBackend` (CLI uses these).
//! All three apps call these same functions. Zero duplication.

use serde::Serialize;

use springtale_core::rule::engine::TriggerEvent;
use springtale_core::rule::types::{Rule, RuleId};
use springtale_store::StorageBackend;

use crate::error::OperationError;
use crate::state::RuntimeState;

/// Maximum rules per instance (DoS prevention).
const MAX_RULES: usize = 10_000;

/// Rule summary for listing.
#[derive(Debug, Serialize)]
pub struct RuleSummary {
    pub id: String,
    pub name: String,
    pub status: String,
    pub trigger_type: String,
    /// Connector name from trigger (if ConnectorEvent trigger).
    pub connector_name: Option<String>,
}

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

/// List all rules from the engine (authoritative source).
pub async fn list_rules(state: &RuntimeState) -> Vec<RuleSummary> {
    let engine = state.engine.read().await;
    engine
        .list_rules()
        .iter()
        .map(|r| RuleSummary {
            id: r.id.to_string(),
            name: r.name.clone(),
            status: format!("{:?}", r.status).to_lowercase(),
            trigger_type: r.trigger.trigger_type().to_owned(),
            connector_name: r.trigger.connector_name(),
        })
        .collect()
}

// ── NL→Rule (AI-assisted rule generation) ───────────────────────────────────

/// Parse natural language intent into a structured Rule via AI adapter.
///
/// Returns the generated Rule in Disabled status (user reviews before enabling).
/// Does NOT persist — caller previews, then calls `create_rule()` to save.
///
/// Uses the adapter's `parse_rule()` method which builds a prompt from the
/// installed connectors' trigger/action metadata, sends to the LLM, and
/// parses the structured response into a typed `Rule`.
pub async fn parse_rule_from_intent(
    state: &RuntimeState,
    intent: &str,
) -> Result<Rule, OperationError> {
    // Build ConnectorInfo list from registry (respecting DataDisclosure).
    // Each connector's triggers and actions are included so the AI knows
    // what's available to compose into rules.
    let registry = state.registry.read().await;
    let available: Vec<springtale_ai::ConnectorInfo> = registry
        .list()
        .iter()
        .filter_map(|(name, _)| {
            let entry = registry.get(name)?;
            let manifest = entry.host.manifest();
            Some(springtale_ai::ConnectorInfo {
                name: manifest.name.clone(),
                description: manifest.description.clone(),
                triggers: manifest
                    .triggers
                    .iter()
                    .map(|t| springtale_ai::adapter::TriggerInfo {
                        name: t.name.clone(),
                        description: t.description.clone(),
                        schema: t.schema.clone(),
                    })
                    .collect(),
                actions: manifest
                    .actions
                    .iter()
                    .map(|a| springtale_ai::adapter::ActionInfo {
                        name: a.name.clone(),
                        description: a.description.clone(),
                        input_schema: a.input_schema.clone(),
                        output_schema: a.output_schema.clone(),
                    })
                    .collect(),
                disclosure_level: springtale_ai::DisclosureLevel::NamesAndDescriptions,
            })
        })
        .collect();
    drop(registry);

    // Call AI adapter's parse_rule method
    let adapter = state.ai_adapter.load();
    let rule = adapter
        .parse_rule(intent, &available)
        .await
        .map_err(|e| OperationError::Ai(format!("{e}")))?;

    tracing::info!(
        rule_name = %rule.name,
        trigger = ?rule.trigger,
        actions = rule.actions.len(),
        "AI generated rule from intent"
    );

    Ok(rule)
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

/// Request to create a connector-event rule with simple fields.
///
/// Frontend sends field names, backend assembles the full Rule struct.
/// Eliminates hardcoded "ConnectorEvent"/"RunConnector" type tags in frontend.
#[derive(Debug, serde::Deserialize)]
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

// ── Connector-scoped queries ────────────────────────────────────────────────

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

/// Test result for a connector.
#[derive(Debug, Serialize)]
pub struct ConnectorTestResult {
    pub matched: bool,
    pub rule_name: Option<String>,
}

/// Test a connector by finding and dry-running its first rule.
///
/// Replaces the frontend two-step: find rule → run rule.
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
            springtale_core::rule::Action::RunConnector {
                action, params, ..
            } => springtale_core::rule::Action::RunConnector {
                connector: new_connector.to_owned(),
                action: action.clone(),
                params: params.clone(),
            },
            other => other.clone(),
        })
        .collect();

    let updated = Rule {
        trigger: new_trigger,
        actions: new_actions,
        ..rule
    };

    update_rule(state, id, updated).await
}
