use tauri::State;

use springtale_core::rule::types::RuleId;

use crate::state::AppState;

/// List all automation rules.
#[tauri::command]
pub async fn list_rules(
    state: State<'_, AppState>,
) -> Result<Vec<springtale_runtime::operations::rules::RuleSummary>, String> {
    Ok(springtale_runtime::operations::rules::list_rules(&state.runtime).await)
}

/// Create a new automation rule.
#[tauri::command]
pub async fn create_rule(
    state: State<'_, AppState>,
    rule: serde_json::Value,
) -> Result<String, String> {
    let rule: springtale_core::rule::types::Rule =
        serde_json::from_value(rule).map_err(|e| format!("invalid rule: {e}"))?;

    let id = springtale_runtime::operations::rules::create_rule(&state.runtime, rule)
        .await
        .map_err(|e| e.to_string())?;

    Ok(id.to_string())
}

/// Toggle a rule's enabled/disabled status.
#[tauri::command]
pub async fn toggle_rule(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    let rule_id: RuleId = id
        .parse::<uuid::Uuid>()
        .map(RuleId)
        .map_err(|e| format!("invalid rule ID: {e}"))?;

    springtale_runtime::operations::rules::toggle_rule(&state.runtime, &rule_id, enabled)
        .await
        .map_err(|e| e.to_string())
}

/// Delete a rule.
#[tauri::command]
pub async fn delete_rule(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let rule_id: RuleId = id
        .parse::<uuid::Uuid>()
        .map(RuleId)
        .map_err(|e| format!("invalid rule ID: {e}"))?;

    springtale_runtime::operations::rules::delete_rule(&state.runtime, &rule_id)
        .await
        .map_err(|e| e.to_string())
}

/// Update a rule (replace).
#[tauri::command]
pub async fn update_rule(
    state: State<'_, AppState>,
    id: String,
    rule: serde_json::Value,
) -> Result<(), String> {
    let rule_id: RuleId = id
        .parse::<uuid::Uuid>()
        .map(RuleId)
        .map_err(|e| format!("invalid rule ID: {e}"))?;

    let rule: springtale_core::rule::types::Rule =
        serde_json::from_value(rule).map_err(|e| format!("invalid rule: {e}"))?;

    springtale_runtime::operations::rules::update_rule(&state.runtime, &rule_id, rule)
        .await
        .map_err(|e| e.to_string())
}

/// Dry-run a rule — evaluate without executing actions.
#[tauri::command]
pub async fn run_rule(
    state: State<'_, AppState>,
    id: String,
) -> Result<springtale_runtime::operations::rules::RunResult, String> {
    let rule_id: RuleId = id
        .parse::<uuid::Uuid>()
        .map(RuleId)
        .map_err(|e| format!("invalid rule ID: {e}"))?;

    springtale_runtime::operations::rules::run_rule(&state.runtime, &rule_id)
        .await
        .map_err(|e| e.to_string())
}
