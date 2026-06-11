use tauri::State;

use springtale_core::rule::types::{Rule, RuleId};
use springtale_runtime::RuntimeState;

use crate::runtime_guard::require_runtime;
use crate::state::AppState;

/// Clone a rule out of the engine by id (`None` if absent). Needed to
/// activate/deactivate its triggers around a store mutation.
async fn lookup_rule(rt: &RuntimeState, rule_id: &RuleId) -> Option<Rule> {
    let engine = rt.engine.read().await;
    engine
        .list_rules()
        .iter()
        .find(|r| &r.id == rule_id)
        .map(|r| (*r).clone())
}

/// Activate a rule's cron/filewatch + ConnectorEvent triggers via the
/// shared runtime lifecycle helper — the SAME activation the daemon
/// performs in its HTTP handlers. No-op if the scheduler/registry
/// aren't up yet (vault still locked).
async fn activate_triggers(state: &AppState, rt: &RuntimeState, rule: &Rule) {
    let sched = state.scheduler.read().await;
    let reg = state.trigger_registry.read().await;
    if let (Some(scheduler), Some(registry)) = (sched.as_ref(), reg.as_ref()) {
        springtale_runtime::activate_rule(rule, scheduler, registry, &rt.registry).await;
    }
}

/// Deactivate a rule's triggers (mirror of [`activate_triggers`]).
async fn deactivate_triggers(state: &AppState, rt: &RuntimeState, rule: &Rule) {
    let sched = state.scheduler.read().await;
    let reg = state.trigger_registry.read().await;
    if let (Some(scheduler), Some(registry)) = (sched.as_ref(), reg.as_ref()) {
        springtale_runtime::deactivate_rule(rule, scheduler, registry, &rt.registry).await;
    }
}

/// Get rule schema — trigger, condition, and action type definitions.
#[tauri::command]
#[specta::specta]
pub async fn get_rule_schema() -> serde_json::Value {
    springtale_runtime::operations::rules::get_rule_schema()
}

/// List all automation rules.
#[tauri::command]
#[specta::specta]
pub async fn list_rules(
    state: State<'_, AppState>,
) -> Result<Vec<springtale_runtime::operations::rules::RuleSummary>, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    Ok(springtale_runtime::operations::rules::list_rules(rt).await)
}

/// Create a new automation rule.
#[tauri::command]
#[specta::specta]
pub async fn create_rule(
    state: State<'_, AppState>,
    rule: serde_json::Value,
) -> Result<String, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();

    let rule: Rule = serde_json::from_value(rule).map_err(|e| format!("invalid rule: {e}"))?;

    let id = springtale_runtime::operations::rules::create_rule(rt, rule)
        .await
        .map_err(|e| e.to_string())?;

    // Activate the new rule's triggers so it actually fires (cron tick,
    // connector event) — not just persisted to the store.
    if let Some(created) = lookup_rule(rt, &id).await {
        activate_triggers(&state, rt, &created).await;
    }

    Ok(id.to_string())
}

/// Toggle a rule's enabled/disabled status.
#[tauri::command]
#[specta::specta]
pub async fn toggle_rule(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();

    let rule_id: RuleId = id
        .parse::<uuid::Uuid>()
        .map(RuleId)
        .map_err(|e| format!("invalid rule ID: {e}"))?;

    springtale_runtime::operations::rules::toggle_rule(rt, &rule_id, enabled)
        .await
        .map_err(|e| e.to_string())?;

    // Activate or deactivate the rule's triggers to match the new state.
    if let Some(rule) = lookup_rule(rt, &rule_id).await {
        if enabled {
            activate_triggers(&state, rt, &rule).await;
        } else {
            deactivate_triggers(&state, rt, &rule).await;
        }
    }
    Ok(())
}

/// Delete a rule.
#[tauri::command]
#[specta::specta]
pub async fn delete_rule(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();

    let rule_id: RuleId = id
        .parse::<uuid::Uuid>()
        .map(RuleId)
        .map_err(|e| format!("invalid rule ID: {e}"))?;

    // Tear down its triggers BEFORE the rule leaves the store.
    if let Some(rule) = lookup_rule(rt, &rule_id).await {
        deactivate_triggers(&state, rt, &rule).await;
    }

    springtale_runtime::operations::rules::delete_rule(rt, &rule_id)
        .await
        .map_err(|e| e.to_string())
}

/// Update a rule (replace).
#[tauri::command]
#[specta::specta]
pub async fn update_rule(
    state: State<'_, AppState>,
    id: String,
    rule: serde_json::Value,
) -> Result<(), String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();

    let rule_id: RuleId = id
        .parse::<uuid::Uuid>()
        .map(RuleId)
        .map_err(|e| format!("invalid rule ID: {e}"))?;

    let rule: Rule = serde_json::from_value(rule).map_err(|e| format!("invalid rule: {e}"))?;

    // Detach the old trigger wiring, swap the rule, re-attach the new —
    // the update = detach-old + attach-new pattern (HA/n8n).
    if let Some(old) = lookup_rule(rt, &rule_id).await {
        deactivate_triggers(&state, rt, &old).await;
    }

    springtale_runtime::operations::rules::update_rule(rt, &rule_id, rule.clone())
        .await
        .map_err(|e| e.to_string())?;

    activate_triggers(&state, rt, &rule).await;
    Ok(())
}

/// Dry-run a rule — evaluate without executing actions.
#[tauri::command]
#[specta::specta]
pub async fn run_rule(
    state: State<'_, AppState>,
    id: String,
) -> Result<springtale_runtime::operations::rules::RunResult, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();

    let rule_id: RuleId = id
        .parse::<uuid::Uuid>()
        .map(RuleId)
        .map_err(|e| format!("invalid rule ID: {e}"))?;

    springtale_runtime::operations::rules::run_rule(rt, &rule_id)
        .await
        .map_err(|e| e.to_string())
}

/// Create a connector-event rule from simple fields (no type tags needed).
///
/// The wire shape `serde_json::Value` mirrors `create_rule` /
/// `update_rule`: the frontend posts a JSON payload, the backend
/// deserializes it into the typed `CreateConnectorRuleRequest`. Going
/// through Value here keeps the recursive `Condition` enum off the
/// specta type-graph (see `springtale_core::rule::types` module doc
/// for the policy).
#[tauri::command]
#[specta::specta]
pub async fn create_connector_rule(
    state: State<'_, AppState>,
    rule: serde_json::Value,
) -> Result<String, String> {
    let req: springtale_runtime::operations::rules::CreateConnectorRuleRequest =
        serde_json::from_value(rule).map_err(|e| e.to_string())?;
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    let id = springtale_runtime::operations::rules::create_connector_rule(rt, req)
        .await
        .map_err(|e| e.to_string())?;

    // Attach the connector event handler so the new rule fires.
    if let Some(created) = lookup_rule(rt, &id).await {
        activate_triggers(&state, rt, &created).await;
    }

    Ok(id.to_string())
}

/// List rules for a specific connector.
#[tauri::command]
#[specta::specta]
pub async fn list_rules_for_connector(
    state: State<'_, AppState>,
    connector_name: String,
) -> Result<Vec<springtale_runtime::operations::rules::RuleSummary>, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    Ok(springtale_runtime::operations::rules::list_rules_for_connector(rt, &connector_name).await)
}

/// Test a connector by dry-running its first rule.
#[tauri::command]
#[specta::specta]
pub async fn test_connector(
    state: State<'_, AppState>,
    connector_name: String,
) -> Result<springtale_runtime::operations::rules::ConnectorTestResult, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::rules::test_connector(rt, &connector_name)
        .await
        .map_err(|e| e.to_string())
}

/// Reassign a rule to a different connector.
#[tauri::command]
#[specta::specta]
pub async fn reassign_rule_connector(
    state: State<'_, AppState>,
    id: String,
    new_connector: String,
) -> Result<(), String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    let rule_id = id
        .parse::<uuid::Uuid>()
        .map(springtale_core::rule::types::RuleId)
        .map_err(|e| format!("invalid rule id: {e}"))?;

    // Detach the old connector's event handler before reassigning.
    if let Some(old) = lookup_rule(rt, &rule_id).await {
        deactivate_triggers(&state, rt, &old).await;
    }

    springtale_runtime::operations::rules::reassign_rule_connector(rt, &rule_id, &new_connector)
        .await
        .map_err(|e| e.to_string())?;

    // Attach the new connector's event handler.
    if let Some(reassigned) = lookup_rule(rt, &rule_id).await {
        activate_triggers(&state, rt, &reassigned).await;
    }
    Ok(())
}

/// Parse natural language intent into a Rule (preview — not persisted).
#[tauri::command]
#[specta::specta]
pub async fn parse_rule(
    state: State<'_, AppState>,
    intent: String,
) -> Result<serde_json::Value, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    let rule = springtale_runtime::operations::rules::parse_rule_from_intent(rt, &intent)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&rule).map_err(|e| e.to_string())
}
