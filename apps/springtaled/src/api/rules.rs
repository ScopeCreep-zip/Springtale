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
#[utoipa::path(
    get, operation_id = "rules_schema",
    path = "/rules/schema",
    tag = "rules",
    responses((status = 200, description = "Rule JSON schema", body = Object))
)]
pub async fn schema() -> impl IntoResponse {
    Json(operations::rules::get_rule_schema())
}

/// GET /rules — list all rules.
#[utoipa::path(
    get, operation_id = "rules_list",
    path = "/rules",
    tag = "rules",
    responses((status = 200, description = "All rules", body = Vec<Object>))
)]
pub async fn list(State(state): State<AppState>) -> impl IntoResponse {
    let rules = operations::rules::list_rules(&state.runtime).await;
    Json(serde_json::json!({ "rules": rules }))
}

/// POST /rules — create a new rule.
#[utoipa::path(
    post, operation_id = "rules_create",
    path = "/rules",
    tag = "rules",
    request_body = Object,
    responses((status = 200, description = "Rule created", body = Object))
)]
pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let rule: springtale_core::rule::types::Rule =
        serde_json::from_value(body).map_err(|_| StatusCode::BAD_REQUEST)?;

    // Persist (store + engine) before activating triggers — if the store
    // insert fails, no trigger is ever scheduled or attached.
    let rule_id = match operations::rules::create_and_activate(
        &state.runtime,
        &state.scheduler,
        &state.trigger_registry,
        rule,
    )
    .await
    {
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
#[utoipa::path(
    put, operation_id = "rules_update",
    path = "/rules/{id}",
    tag = "rules",
    params(("id" = String, Path, description = "Rule id")),
    request_body = Object,
    responses((status = 200, description = "Rule updated", body = Object))
)]
pub async fn update(
    State(state): State<AppState>,
    ValidatedPath(id): ValidatedPath,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let uuid = uuid::Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let rule_id = springtale_core::rule::types::RuleId(uuid);

    let rule: springtale_core::rule::types::Rule =
        serde_json::from_value(body).map_err(|_| StatusCode::BAD_REQUEST)?;

    // Deactivate old triggers, persist the update, activate the new triggers.
    operations::rules::update_and_reactivate(
        &state.runtime,
        &state.scheduler,
        &state.trigger_registry,
        &rule_id,
        rule,
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to update rule");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((StatusCode::OK, Json(serde_json::json!({ "updated": id }))))
}

/// DELETE /rules/{id} — delete a rule.
#[utoipa::path(
    delete, operation_id = "rules_delete",
    path = "/rules/{id}",
    tag = "rules",
    params(("id" = String, Path, description = "Rule id")),
    responses((status = 200, description = "Rule deleted", body = Object))
)]
pub async fn delete(
    State(state): State<AppState>,
    ValidatedPath(id): ValidatedPath,
) -> Result<impl IntoResponse, StatusCode> {
    let uuid = uuid::Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let rule_id = springtale_core::rule::types::RuleId(uuid);

    // Deactivate triggers, then delete (store + engine).
    operations::rules::delete_and_deactivate(
        &state.runtime,
        &state.scheduler,
        &state.trigger_registry,
        &rule_id,
    )
    .await
    .map_err(|_| StatusCode::NOT_FOUND)?;

    Ok((StatusCode::OK, Json(serde_json::json!({ "deleted": id }))))
}

/// POST /rules/{id}/toggle — toggle a rule's enabled/disabled status.
#[utoipa::path(
    post, operation_id = "rules_toggle",
    path = "/rules/{id}/toggle",
    tag = "rules",
    params(("id" = String, Path, description = "Rule id")),
    request_body = Object,
    responses((status = 200, description = "Rule enabled/disabled", body = Object))
)]
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
#[utoipa::path(
    post, operation_id = "rules_run",
    path = "/rules/{id}/run",
    tag = "rules",
    params(("id" = String, Path, description = "Rule id")),
    responses((status = 200, description = "Rule run once", body = Object))
)]
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
#[utoipa::path(
    post, operation_id = "rules_parse",
    path = "/rules/parse",
    tag = "rules",
    request_body = Object,
    responses((status = 200, description = "Parsed rule", body = Object))
)]
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
#[utoipa::path(
    post, operation_id = "rules_create_connector_rule",
    path = "/rules/connector",
    tag = "rules",
    request_body = operations::rules::CreateConnectorRuleRequest,
    responses((status = 200, description = "Connector rule created", body = Object))
)]
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
#[utoipa::path(
    get, operation_id = "rules_list_for_connector",
    path = "/rules/connector/{name}",
    tag = "rules",
    params(("name" = String, Path, description = "Connector name")),
    responses((status = 200, description = "Rules bound to one connector", body = Vec<Object>))
)]
pub async fn list_for_connector(
    State(state): State<AppState>,
    ValidatedPath(name): ValidatedPath,
) -> Result<impl IntoResponse, StatusCode> {
    let rules = operations::rules::list_rules_for_connector(&state.runtime, &name).await;
    Ok(Json(serde_json::json!({ "rules": rules })))
}

/// POST /connectors/{name}/test — test a connector by dry-running its first rule.
#[utoipa::path(
    post, operation_id = "rules_test_connector",
    path = "/connectors/{name}/test",
    tag = "rules",
    params(("name" = String, Path, description = "Connector name")),
    responses((status = 200, description = "Connector test outcome", body = Object))
)]
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
#[utoipa::path(
    post, operation_id = "rules_reassign",
    path = "/rules/{id}/reassign",
    tag = "rules",
    params(("id" = String, Path, description = "Rule id")),
    request_body = Object,
    responses((status = 200, description = "Rule reassigned", body = Object))
)]
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
