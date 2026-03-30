use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

use springtale_store::backend::trait_::StorageBackend;

use super::state::AppState;

/// Maximum number of rules per instance. Prevents O(n) rule evaluation from
/// becoming a DoS vector when combined with high event rates.
const MAX_RULES: usize = 10_000;

/// GET /rules — list all rules.
pub async fn list(State(state): State<AppState>) -> impl IntoResponse {
    let engine = state.engine.read().await;
    let rules: Vec<serde_json::Value> = engine
        .list_rules()
        .iter()
        .map(|rule| {
            serde_json::json!({
                "id": rule.id.to_string(),
                "name": rule.name,
                "status": format!("{:?}", rule.status),
                "trigger_type": format!("{:?}", rule.trigger),
            })
        })
        .collect();

    Json(serde_json::json!({ "rules": rules }))
}

/// POST /rules — create a new rule.
///
/// Accepts a JSON rule definition. Adds to both the engine and the store.
pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    // Phase 1a: parse rule from JSON body
    let rule: springtale_core::rule::types::Rule =
        serde_json::from_value(body).map_err(|_| StatusCode::BAD_REQUEST)?;

    // Check rule count limit before inserting
    {
        let engine = state.engine.read().await;
        if engine.list_rules().len() >= MAX_RULES {
            tracing::warn!("rule count limit reached ({MAX_RULES})");
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
    }

    let rule_id = rule.id;

    // Add to store
    state
        .store
        .insert_rule(&rule)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Schedule trigger if applicable — if this fails, roll back the store insert
    if let Err(e) = schedule_rule_trigger(&state, &rule).await {
        tracing::error!(rule = %rule.name, error = %e, "trigger scheduling failed, rolling back");
        let _ = state.store.delete_rule(&rule_id).await;
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    // Add to engine
    {
        let mut engine = state.engine.write().await;
        if let Err(e) = engine.add_rule(rule) {
            tracing::error!(error = %e, "failed to add rule to engine");
            let _ = state.store.delete_rule(&rule_id).await;
            return Err(StatusCode::BAD_REQUEST);
        }
    }

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

    let mut rule: springtale_core::rule::types::Rule =
        serde_json::from_value(body).map_err(|_| StatusCode::BAD_REQUEST)?;

    // Force the rule ID to match the URL path — prevents ID mismatch
    rule.id = rule_id;

    // Insert new rule FIRST — if this fails, old rule is still intact in store.
    // This prevents data loss if the insert fails after deleting the old rule.
    state
        .store
        .insert_rule(&rule)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Now safe to delete old rule from store (new one is persisted)
    let _ = state.store.delete_rule(&rule_id).await;

    // Unschedule old triggers, schedule new ones.
    // Clone the rule data under lock, then drop lock before awaiting unschedule
    // to avoid holding the engine read lock across the cron Mutex await.
    let old_rule = {
        let engine = state.engine.read().await;
        engine
            .list_rules()
            .iter()
            .find(|r| r.id == rule_id)
            .map(|r| (*r).clone())
    };
    if let Some(old_rule) = old_rule {
        unschedule_rule_trigger(&state, &old_rule).await;
    }

    if let Err(e) = schedule_rule_trigger(&state, &rule).await {
        tracing::warn!(rule = %rule.name, error = %e, "failed to schedule updated rule trigger");
    }

    // Update engine
    {
        let mut engine = state.engine.write().await;
        engine.remove_rule(&rule_id);
        if let Err(e) = engine.add_rule(rule) {
            tracing::error!(error = %e, "failed to add updated rule to engine");
            return Err(StatusCode::BAD_REQUEST);
        }
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

    // Unschedule triggers before deleting.
    // Clone rule data under lock, drop lock before awaiting unschedule.
    let old_rule = {
        let engine = state.engine.read().await;
        engine
            .list_rules()
            .iter()
            .find(|r| r.id == rule_id)
            .map(|r| (*r).clone())
    };
    if let Some(old_rule) = old_rule {
        unschedule_rule_trigger(&state, &old_rule).await;
    }

    state
        .store
        .delete_rule(&rule_id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    // Remove from engine
    {
        let mut engine = state.engine.write().await;
        engine.remove_rule(&rule_id);
    }

    Ok((StatusCode::OK, Json(serde_json::json!({ "deleted": id }))))
}

/// POST /rules/{id}/run — manually trigger a rule.
pub async fn run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    super::validate_path_param(&id)?;
    let uuid = uuid::Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let rule_id = springtale_core::rule::types::RuleId(uuid);

    let engine = state.engine.read().await;

    // Find the rule
    let rule = engine
        .list_rules()
        .into_iter()
        .find(|r| r.id == rule_id)
        .ok_or(StatusCode::NOT_FOUND)?;

    // Create a synthetic trigger event that matches the rule's trigger definition.
    // Each trigger type requires specific fields to match (see trigger_matches in engine.rs).
    let event = match &rule.trigger {
        springtale_core::rule::Trigger::Cron { .. } => {
            springtale_core::rule::engine::TriggerEvent {
                trigger_type: "Cron".to_owned(),
                connector: None,
                event: None,
                payload: serde_json::json!({"manual_trigger": true}),
            }
        }
        springtale_core::rule::Trigger::FileWatch { path, event: ev } => {
            springtale_core::rule::engine::TriggerEvent {
                trigger_type: "FileWatch".to_owned(),
                connector: None,
                event: Some(format!("{path}:{ev}")),
                payload: serde_json::json!({"manual_trigger": true, "path": path}),
            }
        }
        springtale_core::rule::Trigger::Webhook { path } => {
            springtale_core::rule::engine::TriggerEvent {
                trigger_type: "Webhook".to_owned(),
                connector: None,
                event: Some(path.clone()),
                payload: serde_json::json!({"manual_trigger": true}),
            }
        }
        springtale_core::rule::Trigger::ConnectorEvent {
            connector,
            event: ev,
        } => springtale_core::rule::engine::TriggerEvent {
            trigger_type: "ConnectorEvent".to_owned(),
            connector: Some(connector.clone()),
            event: Some(ev.clone()),
            payload: serde_json::json!({"manual_trigger": true}),
        },
        springtale_core::rule::Trigger::SystemEvent { event: ev } => {
            springtale_core::rule::engine::TriggerEvent {
                trigger_type: "SystemEvent".to_owned(),
                connector: None,
                event: Some(ev.clone()),
                payload: serde_json::json!({"manual_trigger": true}),
            }
        }
    };

    let matches = engine.evaluate(&event);

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "rule_id": id,
            "matched": !matches.is_empty(),
            "actions_count": matches.iter().map(|m| m.actions.len()).sum::<usize>(),
        })),
    ))
}

/// Schedule a rule's trigger in the cron executor or file watcher.
///
/// Called when a rule is created or updated via the API. Without this,
/// cron and FileWatch triggers added at runtime would never fire.
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
        _ => {} // Other trigger types don't need scheduler registration
    }
    Ok(())
}

/// Unschedule a rule's trigger from the cron executor or file watcher.
///
/// Called when a rule is updated or deleted via the API. Without this,
/// deleted/changed cron jobs and file watches continue firing.
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
