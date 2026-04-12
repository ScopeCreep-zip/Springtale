use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use springtale_runtime::operations;

use super::extractors::ValidatedPath;
use super::state::AppState;

/// GET /rules/schema — return JSON schemas for trigger, condition, and action types.
///
/// Used by the visual rule builder to generate input forms for each type.
pub async fn schema() -> impl IntoResponse {
    Json(operations::rules::get_rule_schema())
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

    // Schedule trigger (app-specific: cron/fs_watcher + connector events)
    if let Err(e) = state.scheduler.schedule(&rule).await {
        tracing::error!(rule = %rule.name, error = %e, "trigger scheduling failed");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    state
        .trigger_registry
        .attach_rule(&rule, &state.runtime.registry)
        .await;

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
    ValidatedPath(id): ValidatedPath,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let uuid = uuid::Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let rule_id = springtale_core::rule::types::RuleId(uuid);

    let rule: springtale_core::rule::types::Rule =
        serde_json::from_value(body).map_err(|_| StatusCode::BAD_REQUEST)?;

    // Unschedule old triggers (app-specific: cron/fs + connector events)
    let old_rule = {
        let engine = state.runtime.engine.read().await;
        engine
            .list_rules()
            .iter()
            .find(|r| r.id == rule_id)
            .map(|r| (*r).clone())
    };
    if let Some(ref old) = old_rule {
        state.scheduler.unschedule(old).await;
    }
    state
        .trigger_registry
        .detach_rule(&rule_id, &state.runtime.registry)
        .await;

    // Delegate store+engine update to operations
    operations::rules::update_rule(&state.runtime, &rule_id, rule.clone())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to update rule");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Schedule new triggers (app-specific: cron/fs + connector events)
    if let Err(e) = state.scheduler.schedule(&rule).await {
        tracing::warn!(rule = %rule.name, error = %e, "failed to schedule updated rule trigger");
    }
    state
        .trigger_registry
        .attach_rule(&rule, &state.runtime.registry)
        .await;

    Ok((StatusCode::OK, Json(serde_json::json!({ "updated": id }))))
}

/// DELETE /rules/{id} — delete a rule.
pub async fn delete(
    State(state): State<AppState>,
    ValidatedPath(id): ValidatedPath,
) -> Result<impl IntoResponse, StatusCode> {
    let uuid = uuid::Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let rule_id = springtale_core::rule::types::RuleId(uuid);

    // Unschedule triggers (app-specific: cron/fs + connector events)
    let old_rule = {
        let engine = state.runtime.engine.read().await;
        engine
            .list_rules()
            .iter()
            .find(|r| r.id == rule_id)
            .map(|r| (*r).clone())
    };
    if let Some(ref old) = old_rule {
        state.scheduler.unschedule(old).await;
    }
    state
        .trigger_registry
        .detach_rule(&rule_id, &state.runtime.registry)
        .await;

    // Delegate store+engine deletion to operations
    operations::rules::delete_rule(&state.runtime, &rule_id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    Ok((StatusCode::OK, Json(serde_json::json!({ "deleted": id }))))
}

/// POST /rules/{id}/toggle — toggle a rule's enabled/disabled status.
pub async fn toggle(
    State(state): State<AppState>,
    ValidatedPath(id): ValidatedPath,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let uuid = uuid::Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let rule_id = springtale_core::rule::types::RuleId(uuid);

    let enabled = body
        .get("enabled")
        .and_then(|v| v.as_bool())
        .ok_or(StatusCode::BAD_REQUEST)?;

    // Get the rule before toggling (for scheduler management)
    let rule = {
        let engine = state.runtime.engine.read().await;
        engine
            .list_rules()
            .iter()
            .find(|r| r.id == rule_id)
            .map(|r| (*r).clone())
    };

    operations::rules::toggle_rule(&state.runtime, &rule_id, enabled)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    // Attach or detach ALL trigger types based on new state
    if enabled {
        if let Some(ref rule) = rule {
            // Schedule cron/fs triggers
            if let Err(e) = state.scheduler.schedule(rule).await {
                tracing::warn!(rule_id = %id, error = %e, "failed to schedule trigger on enable");
            }
            // Attach connector event handlers
            state
                .trigger_registry
                .attach_rule(rule, &state.runtime.registry)
                .await;
        }
    } else {
        if let Some(ref rule) = rule {
            // Unschedule cron/fs triggers
            state.scheduler.unschedule(rule).await;
        }
        // Detach connector event handlers
        state
            .trigger_registry
            .detach_rule(&rule_id, &state.runtime.registry)
            .await;
    }

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "id": id, "enabled": enabled })),
    ))
}

/// POST /rules/{id}/run — manually trigger a rule (dry-run).
pub async fn run(
    State(state): State<AppState>,
    ValidatedPath(id): ValidatedPath,
) -> Result<impl IntoResponse, StatusCode> {
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

/// POST /rules/parse — generate a Rule from natural language intent.
///
/// Returns the generated Rule for preview (not persisted).
/// The frontend shows the preview and the user calls POST /rules to save.
pub async fn parse(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let intent = body
        .get("intent")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let rule = operations::rules::parse_rule_from_intent(&state.runtime, intent)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "parse_rule_from_intent failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let rule_json = serde_json::to_value(&rule).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({ "rule": rule_json })))
}

/// POST /rules/connector — create a connector-event rule from simple fields.
pub async fn create_connector_rule(
    State(state): State<AppState>,
    Json(req): Json<operations::rules::CreateConnectorRuleRequest>,
) -> impl IntoResponse {
    match operations::rules::create_connector_rule(&state.runtime, req).await {
        Ok(id) => {
            // Attach connector event handler for the new rule
            let rule = {
                let engine = state.runtime.engine.read().await;
                engine
                    .list_rules()
                    .iter()
                    .find(|r| r.id == id)
                    .map(|r| (*r).clone())
            };
            if let Some(rule) = rule {
                state
                    .trigger_registry
                    .attach_rule(&rule, &state.runtime.registry)
                    .await;
            }
            (
                StatusCode::CREATED,
                Json(serde_json::json!({ "id": id.to_string() })),
            )
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// GET /rules/connector/{name} — list rules for a specific connector.
pub async fn list_for_connector(
    State(state): State<AppState>,
    ValidatedPath(name): ValidatedPath,
) -> Result<impl IntoResponse, StatusCode> {
    let rules = operations::rules::list_rules_for_connector(&state.runtime, &name).await;
    Ok(Json(serde_json::json!({ "rules": rules })))
}

/// POST /connectors/{name}/test — test a connector by dry-running its first rule.
pub async fn test_connector(
    State(state): State<AppState>,
    ValidatedPath(name): ValidatedPath,
) -> Result<impl IntoResponse, StatusCode> {
    let result = operations::rules::test_connector(&state.runtime, &name)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok(Json(serde_json::json!(result)))
}

/// POST /rules/{id}/reassign — reassign a rule to a new connector.
pub async fn reassign(
    State(state): State<AppState>,
    ValidatedPath(id): ValidatedPath,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let new_connector = body["new_connector"]
        .as_str()
        .ok_or(StatusCode::BAD_REQUEST)?;
    let rule_id = id
        .parse::<uuid::Uuid>()
        .map(springtale_core::rule::types::RuleId)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    // Detach old connector event handlers before reassignment
    state
        .trigger_registry
        .detach_rule(&rule_id, &state.runtime.registry)
        .await;

    operations::rules::reassign_rule_connector(&state.runtime, &rule_id, new_connector)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    // Attach new connector event handlers after reassignment
    let rule = {
        let engine = state.runtime.engine.read().await;
        engine
            .list_rules()
            .iter()
            .find(|r| r.id == rule_id)
            .map(|r| (*r).clone())
    };
    if let Some(rule) = rule {
        state
            .trigger_registry
            .attach_rule(&rule, &state.runtime.registry)
            .await;
    }

    Ok(Json(serde_json::json!({ "reassigned": id })))
}

// Trigger scheduling delegated to crate::scheduler::AppScheduler
// — reusable across handlers, independently testable.
