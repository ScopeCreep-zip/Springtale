use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

use springtale_runtime::operations;

use super::state::AppState;

/// GET /rules/schema — return JSON schemas for trigger, condition, and action types.
///
/// Used by the visual rule builder to generate input forms for each type.
pub async fn schema() -> impl IntoResponse {
    Json(serde_json::json!({
        "triggers": {
            "Cron": { "fields": { "expression": { "type": "string", "description": "Cron expression (6 fields)" } } },
            "FileWatch": { "fields": { "path": { "type": "string" }, "event": { "type": "string", "enum": ["create", "modify", "delete"] } } },
            "Webhook": { "fields": { "path": { "type": "string" } } },
            "ConnectorEvent": { "fields": { "connector": { "type": "string" }, "event": { "type": "string" } } },
            "SystemEvent": { "fields": { "event": { "type": "string" } } },
            "Heartbeat": { "fields": {} },
        },
        "conditions": {
            "FieldEquals": { "fields": { "field": { "type": "string" }, "value": { "type": "any" } } },
            "Contains": { "fields": { "field": { "type": "string" }, "value": { "type": "string" } } },
            "Regex": { "fields": { "field": { "type": "string" }, "pattern": { "type": "string" } } },
            "TimeInRange": { "fields": { "start": { "type": "string", "description": "HH:MM" }, "end": { "type": "string" } } },
            "DayOfWeek": { "fields": { "days": { "type": "array", "items": { "type": "integer", "min": 0, "max": 6 } } } },
        },
        "actions": {
            "RunConnector": { "fields": { "connector": { "type": "string" }, "action": { "type": "string" }, "params": { "type": "object" } } },
            "SendMessage": { "fields": { "text": { "type": "string" } } },
            "WriteFile": { "fields": { "destination": { "type": "string" }, "content": { "type": "string" } } },
            "Notify": { "fields": { "title": { "type": "string" }, "body": { "type": "string" } } },
            "Delay": { "fields": { "seconds": { "type": "integer" } } },
            "AiComplete": { "fields": { "prompt": { "type": "string" }, "adapter": { "type": "string", "optional": true } } },
        },
    }))
}

/// GET /rules — list all rules.
pub async fn list(State(state): State<AppState>) -> impl IntoResponse {
    let rules = operations::rules::list_rules(&state.runtime).await;
    Json(serde_json::json!({ "rules": rules }))
}

/// POST /rules — create a new rule.
pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let rule: springtale_core::rule::types::Rule =
        serde_json::from_value(body).map_err(|_| StatusCode::BAD_REQUEST)?;

    // Schedule trigger (app-specific: cron/fs_watcher)
    if let Err(e) = schedule_rule_trigger(&state, &rule).await {
        tracing::error!(rule = %rule.name, error = %e, "trigger scheduling failed");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let rule_id = match operations::rules::create_rule(&state.runtime, rule).await {
        Ok(id) => id,
        Err(e) => {
            tracing::error!(error = %e, "failed to create rule");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": rule_id.to_string() })),
    ))
}

/// PUT /rules/{id} — update a rule (replace).
pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    super::validate_path_param(&id)?;
    let uuid = uuid::Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let rule_id = springtale_core::rule::types::RuleId(uuid);

    let rule: springtale_core::rule::types::Rule =
        serde_json::from_value(body).map_err(|_| StatusCode::BAD_REQUEST)?;

    // Unschedule old triggers (app-specific)
    let old_rule = {
        let engine = state.runtime.engine.read().await;
        engine
            .list_rules()
            .iter()
            .find(|r| r.id == rule_id)
            .map(|r| (*r).clone())
    };
    if let Some(ref old) = old_rule {
        unschedule_rule_trigger(&state, old).await;
    }

    // Delegate store+engine update to operations
    operations::rules::update_rule(&state.runtime, &rule_id, rule.clone())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to update rule");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Schedule new triggers (app-specific)
    if let Err(e) = schedule_rule_trigger(&state, &rule).await {
        tracing::warn!(rule = %rule.name, error = %e, "failed to schedule updated rule trigger");
    }

    Ok((StatusCode::OK, Json(serde_json::json!({ "updated": id }))))
}

/// DELETE /rules/{id} — delete a rule.
pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    super::validate_path_param(&id)?;
    let uuid = uuid::Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let rule_id = springtale_core::rule::types::RuleId(uuid);

    // Unschedule triggers (app-specific)
    let old_rule = {
        let engine = state.runtime.engine.read().await;
        engine
            .list_rules()
            .iter()
            .find(|r| r.id == rule_id)
            .map(|r| (*r).clone())
    };
    if let Some(ref old) = old_rule {
        unschedule_rule_trigger(&state, old).await;
    }

    // Delegate store+engine deletion to operations
    operations::rules::delete_rule(&state.runtime, &rule_id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    Ok((StatusCode::OK, Json(serde_json::json!({ "deleted": id }))))
}

/// POST /rules/{id}/toggle — toggle a rule's enabled/disabled status.
pub async fn toggle(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    super::validate_path_param(&id)?;
    let uuid = uuid::Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let rule_id = springtale_core::rule::types::RuleId(uuid);

    let enabled = body
        .get("enabled")
        .and_then(|v| v.as_bool())
        .ok_or(StatusCode::BAD_REQUEST)?;

    operations::rules::toggle_rule(&state.runtime, &rule_id, enabled)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "id": id, "enabled": enabled })),
    ))
}

/// POST /rules/{id}/run — manually trigger a rule (dry-run).
pub async fn run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    super::validate_path_param(&id)?;
    let uuid = uuid::Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let rule_id = springtale_core::rule::types::RuleId(uuid);

    let result = operations::rules::run_rule(&state.runtime, &rule_id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "rule_id": id,
            "matched": result.matched,
            "actions_count": result.actions_count,
        })),
    ))
}

/// Schedule a rule's trigger in the cron executor or file watcher.
///
/// App-specific: cron and fs_watcher are in AppState, not RuntimeState.
async fn schedule_rule_trigger(
    state: &AppState,
    rule: &springtale_core::rule::types::Rule,
) -> Result<(), String> {
    match &rule.trigger {
        springtale_core::rule::Trigger::Cron { expression } => {
            let mut cron = state.cron.lock().await;
            cron.schedule(&rule.name, expression)
                .map_err(|e| format!("failed to schedule cron trigger: {e}"))?;
        }
        springtale_core::rule::Trigger::FileWatch { path, .. } => {
            let mut watcher = state.fs_watcher.lock().await;
            watcher
                .watch(path)
                .map_err(|e| format!("failed to watch path: {e}"))?;
        }
        _ => {}
    }
    Ok(())
}

/// Unschedule a rule's trigger from the cron executor or file watcher.
async fn unschedule_rule_trigger(state: &AppState, rule: &springtale_core::rule::types::Rule) {
    match &rule.trigger {
        springtale_core::rule::Trigger::Cron { .. } => {
            let mut cron = state.cron.lock().await;
            if cron.cancel(&rule.name) {
                tracing::info!(rule = %rule.name, "cancelled cron trigger");
            }
        }
        springtale_core::rule::Trigger::FileWatch { path, .. } => {
            let mut watcher = state.fs_watcher.lock().await;
            if let Err(e) = watcher.unwatch(path) {
                tracing::warn!(rule = %rule.name, error = %e, "failed to unwatch path");
            }
        }
        _ => {}
    }
}
