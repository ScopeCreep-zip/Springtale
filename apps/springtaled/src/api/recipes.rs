//! HTTP routes for the W1.B recipe library.
//!
//! Same operations the desktop Tauri commands call — mirrored over
//! HTTP for the web dashboard and any future remote client. Per the
//! architecture invariant, both surfaces consume the same backend
//! ops.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};

use springtale_runtime::operations::preflight::{self, PreflightReport};
use springtale_runtime::operations::preview::{self, PreviewReport};
use springtale_runtime::operations::recipes::{
    self, ApplyReport, Recipe, RecipeCategory, RecipeFilter, RecipeInputs, RecipePieceSummary,
    RecipeSort, RecipeSourceFilter,
};
use springtale_runtime::operations::test_step as test_step_ops;

use super::state::AppState;

/// Query parameters for `GET /recipes`. Mirrors the fields of
/// `RecipeFilter` but flat so curl / fetch can compose URLs without
/// needing a nested JSON body.
#[derive(Debug, Default, Deserialize, utoipa::IntoParams)]
pub struct RecipeListQuery {
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub category: Option<RecipeCategory>,
    #[serde(default)]
    pub tags: Option<String>,
    #[serde(default)]
    pub sources: Option<String>,
    #[serde(default)]
    pub favorites_only: Option<bool>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub sort: Option<RecipeSort>,
}

impl RecipeListQuery {
    fn into_filter(self) -> RecipeFilter {
        let tags = self
            .tags
            .map(|s| {
                s.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        let sources = self
            .sources
            .map(|s| {
                s.split(',')
                    .map(str::trim)
                    .filter_map(|s| match s {
                        "builtin" => Some(RecipeSourceFilter::Builtin),
                        "user" => Some(RecipeSourceFilter::User),
                        "community" => Some(RecipeSourceFilter::Community),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        RecipeFilter {
            query: self.query,
            category: self.category,
            tags,
            sources,
            favorites_only: self.favorites_only.unwrap_or(false),
            limit: self.limit,
            sort: self.sort.unwrap_or_default(),
        }
    }
}

/// GET /recipes — list with optional filter via query params.
#[utoipa::path(
    get, operation_id = "recipes_list",
    path = "/recipes",
    tag = "recipes",
    params(RecipeListQuery),
    responses((status = 200, description = "Matching recipes", body = Vec<Recipe>))
)]
pub async fn list(
    State(state): State<AppState>,
    Query(query): Query<RecipeListQuery>,
) -> Result<Json<Vec<Recipe>>, (StatusCode, String)> {
    let recipes = recipes::list_recipes(&*state.runtime.store, query.into_filter())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(recipes))
}

/// GET /recipes/categories — render the sidebar from this. Server is
/// authoritative on the category list (no frontend hardcoding).
#[utoipa::path(
    get, operation_id = "recipes_list_categories",
    path = "/recipes/categories",
    tag = "recipes",
    responses((status = 200, description = "Recipe categories", body = Vec<Object>))
)]
pub async fn list_categories() -> impl IntoResponse {
    Json(recipes::list_categories())
}

/// GET /recipes/{id} — fetch one. 404 if not found.
#[utoipa::path(
    get, operation_id = "recipes_get_one",
    path = "/recipes/{id}",
    tag = "recipes",
    params(("id" = String, Path, description = "Recipe id")),
    responses((status = 200, description = "One recipe", body = Recipe))
)]
pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Recipe>, (StatusCode, String)> {
    match recipes::get_recipe(&*state.runtime.store, &id).await {
        Ok(Some(r)) => Ok(Json(r)),
        Ok(None) => Err((StatusCode::NOT_FOUND, format!("recipe '{id}' not found"))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct FavoriteResponse {
    pub recipe_id: String,
    pub now_favorite: bool,
}

/// POST /recipes/{id}/favorite — toggle a recipe in/out of favorites.
#[utoipa::path(
    post, operation_id = "recipes_toggle_favorite",
    path = "/recipes/{id}/favorite",
    tag = "recipes",
    params(("id" = String, Path, description = "Recipe id")),
    responses((status = 200, description = "New favorite state", body = FavoriteResponse))
)]
pub async fn toggle_favorite(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<FavoriteResponse>, (StatusCode, String)> {
    let now_favorite = recipes::toggle_favorite(&*state.runtime.store, &id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(FavoriteResponse {
        recipe_id: id,
        now_favorite,
    }))
}

/// POST /recipes/{id}/recent — push onto the recently-used list.
#[utoipa::path(
    post, operation_id = "recipes_record_recent",
    path = "/recipes/{id}/recent",
    tag = "recipes",
    params(("id" = String, Path, description = "Recipe id")),
    responses((status = 200, description = "Recorded"))
)]
pub async fn record_recent(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    recipes::record_recent(&*state.runtime.store, &id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /recipes/{id}/apply — W1.C deploy with user inputs.
#[utoipa::path(
    post, operation_id = "recipes_apply",
    path = "/recipes/{id}/apply",
    tag = "recipes",
    params(("id" = String, Path, description = "Recipe id")),
    request_body = RecipeInputs,
    responses((status = 200, description = "Recipe applied", body = ApplyReport))
)]
pub async fn apply(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(inputs): Json<RecipeInputs>,
) -> Result<Json<ApplyReport>, (StatusCode, String)> {
    let report = recipes::apply_recipe(&state.runtime, &id, inputs)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    // Activate every freshly-created rule's triggers — schedule
    // cron/filewatch AND attach ConnectorEvent handlers (the shared
    // `activate_rule`). `apply_recipe` only persists to the store +
    // engine; without this a deployed recipe never fires.
    let rules = state
        .runtime
        .store
        .list_rules()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    for rule_id in &report.rules_created {
        if let Some(rule) = rules.iter().find(|r| r.id.0.to_string() == *rule_id) {
            springtale_runtime::activate_rule(
                rule,
                &state.scheduler,
                &state.trigger_registry,
                &state.runtime.registry,
            )
            .await;
        }
    }

    let _ = recipes::record_recent(&*state.runtime.store, &id).await;
    Ok(Json(report))
}

/// POST /recipes/{id}/render — show-as-code disclosure renders the
/// assembled TOML for the user's current inputs. Read-only; takes
/// the same payload as `/apply` but doesn't materialise.
#[utoipa::path(
    post, operation_id = "recipes_render",
    path = "/recipes/{id}/render",
    tag = "recipes",
    params(("id" = String, Path, description = "Recipe id")),
    request_body = RecipeInputs,
    responses((status = 200, description = "Rendered rule TOML", body = String))
)]
pub async fn render(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(inputs): Json<RecipeInputs>,
) -> Result<String, (StatusCode, String)> {
    recipes::render_blueprint_toml(&state.runtime, &id, inputs)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))
}

/// POST /recipes/{id}/preflight — W1.D Deploy-readiness checklist.
#[utoipa::path(
    post, operation_id = "recipes_preflight",
    path = "/recipes/{id}/preflight",
    tag = "recipes",
    params(("id" = String, Path, description = "Recipe id")),
    request_body = RecipeInputs,
    responses((status = 200, description = "Preflight report", body = PreflightReport))
)]
pub async fn preflight(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(inputs): Json<RecipeInputs>,
) -> Result<Json<PreflightReport>, (StatusCode, String)> {
    let report = preflight::preflight_recipe(&state.runtime, &id, inputs)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(report))
}

/// POST /recipes/{id}/preview — W2.C dry-run with comic-strip narrative.
#[utoipa::path(
    post, operation_id = "recipes_preview",
    path = "/recipes/{id}/preview",
    tag = "recipes",
    params(("id" = String, Path, description = "Recipe id")),
    request_body = RecipeInputs,
    responses((status = 200, description = "Preview report", body = PreviewReport))
)]
pub async fn preview(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(inputs): Json<RecipeInputs>,
) -> Result<Json<PreviewReport>, (StatusCode, String)> {
    let report = preview::preview_recipe(&state.runtime, &id, inputs)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(report))
}

/// GET /recipes/{id}/pieces — W2.D borrowable slots.
#[utoipa::path(
    get, operation_id = "recipes_list_pieces",
    path = "/recipes/{id}/pieces",
    tag = "recipes",
    params(("id" = String, Path, description = "Recipe id")),
    responses((status = 200, description = "Recipe pieces", body = Vec<RecipePieceSummary>))
)]
pub async fn list_pieces(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<RecipePieceSummary>>, (StatusCode, String)> {
    let pieces = recipes::list_pieces(&state.runtime, &id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(pieces))
}

// ── W2.B Recipe authoring ─────────────────────────────────────

/// POST /recipes/user — save to the user library. Runs Clear Check.
#[utoipa::path(
    post, operation_id = "recipes_save_user",
    path = "/recipes/user",
    tag = "recipes",
    request_body = Recipe,
    responses((status = 200, description = "Saved user recipe", body = Recipe))
)]
pub async fn save_user(
    State(state): State<AppState>,
    Json(recipe): Json<Recipe>,
) -> Result<Json<Recipe>, (StatusCode, String)> {
    let saved = springtale_runtime::operations::recipes::authoring::save_user_recipe(
        &*state.runtime.store,
        recipe,
    )
    .await
    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(saved))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ForkBody {
    pub new_name: String,
}

/// POST /recipes/{id}/fork — duplicate into the user library.
#[utoipa::path(
    post, operation_id = "recipes_fork",
    path = "/recipes/{id}/fork",
    tag = "recipes",
    params(("id" = String, Path, description = "Recipe id")),
    request_body = ForkBody,
    responses((status = 200, description = "Forked recipe", body = Recipe))
)]
pub async fn fork(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ForkBody>,
) -> Result<Json<Recipe>, (StatusCode, String)> {
    let original = recipes::get_recipe(&*state.runtime.store, &id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, format!("recipe '{id}' not found")))?;
    let forked = springtale_runtime::operations::recipes::authoring::fork_recipe(
        &*state.runtime.store,
        &original,
        body.new_name,
    )
    .await
    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(forked))
}

/// DELETE /recipes/user/{id} — remove a user recipe.
#[utoipa::path(
    delete, operation_id = "recipes_delete_user",
    path = "/recipes/user/{id}",
    tag = "recipes",
    params(("id" = String, Path, description = "Recipe id")),
    responses((status = 200, description = "Deleted"))
)]
pub async fn delete_user(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let removed = springtale_runtime::operations::recipes::authoring::delete_user_recipe(
        &*state.runtime.store,
        &id,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if removed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            format!("user recipe '{id}' not found"),
        ))
    }
}

/// GET /recipes/{id}/export — TOML for sharing.
#[utoipa::path(
    get, operation_id = "recipes_export_toml",
    path = "/recipes/{id}/export",
    tag = "recipes",
    params(("id" = String, Path, description = "Recipe id")),
    responses((status = 200, description = "Recipe TOML", body = String))
)]
pub async fn export_toml(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<String, (StatusCode, String)> {
    let recipe = recipes::get_recipe(&*state.runtime.store, &id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, format!("recipe '{id}' not found")))?;
    springtale_runtime::operations::recipes::authoring::export_recipe_toml(&recipe)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// POST /recipes/import — import a recipe from raw TOML.
#[utoipa::path(
    post, operation_id = "recipes_import_toml",
    path = "/recipes/import",
    tag = "recipes",
    request_body = String,
    responses((status = 200, description = "Imported recipe", body = Recipe))
)]
pub async fn import_toml(
    State(state): State<AppState>,
    payload: String,
) -> Result<Json<Recipe>, (StatusCode, String)> {
    let recipe = springtale_runtime::operations::recipes::authoring::import_recipe_toml(
        &*state.runtime.store,
        &payload,
    )
    .await
    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(recipe))
}

/// Body for `POST /recipes/{id}/test-step`.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct TestStepBody {
    pub inputs: RecipeInputs,
    pub rule_index: usize,
    pub step_index: usize,
}

/// POST /recipes/{id}/test-step — Phase C "Test This Step" dry run
/// through `step_index` of `rule_index`.
#[utoipa::path(
    post, operation_id = "recipes_test_step",
    path = "/recipes/{id}/test-step",
    tag = "recipes",
    params(("id" = String, Path, description = "Recipe id")),
    request_body = TestStepBody,
    responses((status = 200, description = "Step test report", body = test_step_ops::TestStepReport))
)]
pub async fn test_step(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<TestStepBody>,
) -> Result<Json<test_step_ops::TestStepReport>, (StatusCode, String)> {
    test_step_ops::test_recipe_step(
        &state.runtime,
        &id,
        body.inputs,
        body.rule_index,
        body.step_index,
    )
    .await
    .map(Json)
    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))
}
