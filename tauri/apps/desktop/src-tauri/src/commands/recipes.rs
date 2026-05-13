//! Tauri commands surfacing the recipe library.
//!
//! Thin wrappers around `springtale_runtime::operations::recipes`.
//! All filtering, sorting, and persistence happens server-side;
//! these commands just relay arguments and serialise results.

use tauri::State;

use springtale_runtime::operations::preflight::{self, PreflightReport};
use springtale_runtime::operations::preview::{self, PreviewReport};
use springtale_runtime::operations::recipes::{
    self, ApplyReport, Recipe, RecipeCategory, RecipeFilter, RecipeInputs,
    RecipePieceSummary,
};
use springtale_runtime::operations::recipes::authoring;

use crate::runtime_guard::require_runtime;
use crate::state::AppState;

#[tauri::command]
#[specta::specta]
pub async fn list_recipes(
    state: State<'_, AppState>,
    filter: Option<RecipeFilter>,
) -> Result<Vec<Recipe>, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    recipes::list_recipes(&*rt.store, filter.unwrap_or_default())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn get_recipe(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<Recipe>, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    recipes::get_recipe(&*rt.store, &id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn list_recipe_categories() -> Vec<RecipeCategory> {
    recipes::list_categories()
}

#[tauri::command]
#[specta::specta]
pub async fn toggle_recipe_favorite(
    state: State<'_, AppState>,
    recipe_id: String,
) -> Result<bool, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    recipes::toggle_favorite(&*rt.store, &recipe_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn record_recipe_recent(
    state: State<'_, AppState>,
    recipe_id: String,
) -> Result<(), String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    recipes::record_recent(&*rt.store, &recipe_id)
        .await
        .map_err(|e| e.to_string())
}

/// W1.C — Deploy a recipe with the user's filled-in inputs.
///
/// In addition to persisting the rules + connector config (handled by
/// `recipes::apply_recipe`), each freshly-created rule is registered
/// with the in-process scheduler so cron triggers actually fire.
/// Mirrors the daemon's `POST /rules` flow which calls
/// `state.scheduler.schedule(&rule)` after `create_rule`.
#[tauri::command]
#[specta::specta]
pub async fn apply_recipe(
    state: State<'_, AppState>,
    recipe_id: String,
    inputs: RecipeInputs,
) -> Result<ApplyReport, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    let report = recipes::apply_recipe(rt, &recipe_id, inputs)
        .await
        .map_err(|e| e.to_string())?;

    // Track E — register every cron / file_watch trigger created by
    // this deploy with the scheduler. Without this, the rule lives
    // in the store + RuleEngine but nothing ticks its trigger.
    if let Some(scheduler) = state.scheduler.read().await.as_ref() {
        let rules = rt
            .store
            .list_rules()
            .await
            .map_err(|e| format!("failed to list rules after apply: {e}"))?;
        for rule_id in &report.rules_created {
            if let Some(rule) = rules.iter().find(|r| r.id.0.to_string() == *rule_id) {
                if let Err(e) = scheduler.schedule(rule).await {
                    tracing::warn!(
                        rule = %rule.name,
                        error = %e,
                        "failed to schedule trigger for newly-applied rule",
                    );
                }
            }
        }
    } else {
        tracing::warn!("scheduler not initialised; new rules will not fire");
    }

    // Track the recipe as recently used so the library reflects it.
    let _ = recipes::record_recent(&*rt.store, &recipe_id).await;
    Ok(report)
}

/// W1.C — Show-as-code disclosure: render the assembled TOML for the
/// user's current inputs without applying. Missing inputs leave their
/// `${placeholder}` visible so the user sees what's not yet filled.
#[tauri::command]
#[specta::specta]
pub async fn render_recipe_toml(
    state: State<'_, AppState>,
    recipe_id: String,
    inputs: RecipeInputs,
) -> Result<String, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    recipes::render_blueprint_toml(rt, &recipe_id, inputs)
        .await
        .map_err(|e| e.to_string())
}

/// W1.D — Preflight checklist before Deploy. Backend decides every
/// row's classification (Blocking / Warning / Verified). Frontend
/// renders the report and consults `deployable` to enable/disable
/// the Deploy button.
#[tauri::command]
#[specta::specta]
pub async fn preflight_recipe(
    state: State<'_, AppState>,
    recipe_id: String,
    inputs: RecipeInputs,
) -> Result<PreflightReport, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    preflight::preflight_recipe(rt, &recipe_id, inputs)
        .await
        .map_err(|e| e.to_string())
}

/// W2.C — Dry-run preview that returns the comic-strip narrative
/// without applying. Used by the deploy form's "Preview" button and
/// the recipe-authoring Clear Check.
#[tauri::command]
#[specta::specta]
pub async fn preview_recipe(
    state: State<'_, AppState>,
    recipe_id: String,
    inputs: RecipeInputs,
) -> Result<PreviewReport, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    preview::preview_recipe(rt, &recipe_id, inputs)
        .await
        .map_err(|e| e.to_string())
}

/// W2.D — Enumerate the named slots a recipe exposes for borrowing
/// into a from-scratch team. Each piece is a self-contained
/// `RuleStep` / `ConnectorConfigStep` / `AiConfigStep` keyed by a
/// stable id the frontend can pass back.
#[tauri::command]
#[specta::specta]
pub async fn list_recipe_pieces(
    state: State<'_, AppState>,
    recipe_id: String,
) -> Result<Vec<RecipePieceSummary>, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    recipes::list_pieces(rt, &recipe_id)
        .await
        .map_err(|e| e.to_string())
}

// ── W2.B Recipe authoring ─────────────────────────────────────

#[tauri::command]
#[specta::specta]
pub async fn save_user_recipe(
    state: State<'_, AppState>,
    recipe: Recipe,
) -> Result<Recipe, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    authoring::save_user_recipe(&*rt.store, recipe)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn fork_recipe(
    state: State<'_, AppState>,
    recipe_id: String,
    new_name: String,
) -> Result<Recipe, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    let original = recipes::get_recipe(&*rt.store, &recipe_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("recipe '{recipe_id}' not found"))?;
    authoring::fork_recipe(&*rt.store, &original, new_name)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn delete_user_recipe(
    state: State<'_, AppState>,
    recipe_id: String,
) -> Result<bool, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    authoring::delete_user_recipe(&*rt.store, &recipe_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn export_recipe_toml(
    state: State<'_, AppState>,
    recipe_id: String,
) -> Result<String, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    let recipe = recipes::get_recipe(&*rt.store, &recipe_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("recipe '{recipe_id}' not found"))?;
    authoring::export_recipe_toml(&recipe).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn import_recipe_toml(
    state: State<'_, AppState>,
    toml_payload: String,
) -> Result<Recipe, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    authoring::import_recipe_toml(&*rt.store, &toml_payload)
        .await
        .map_err(|e| e.to_string())
}
