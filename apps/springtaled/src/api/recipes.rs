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
    self, ApplyReport, Recipe, RecipeCategory, RecipeFilter, RecipeInputs,
    RecipePieceSummary, RecipeSort, RecipeSourceFilter,
};

use super::state::AppState;

/// Query parameters for `GET /recipes`. Mirrors the fields of
/// `RecipeFilter` but flat so curl / fetch can compose URLs without
/// needing a nested JSON body.
#[derive(Debug, Default, Deserialize)]
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
pub async fn list_categories() -> impl IntoResponse {
    Json(recipes::list_categories())
}

/// GET /recipes/{id} — fetch one. 404 if not found.
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

#[derive(Serialize)]
pub struct FavoriteResponse {
    pub recipe_id: String,
    pub now_favorite: bool,
}

/// POST /recipes/{id}/favorite — toggle a recipe in/out of favorites.
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
pub async fn apply(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(inputs): Json<RecipeInputs>,
) -> Result<Json<ApplyReport>, (StatusCode, String)> {
    let report = recipes::apply_recipe(&state.runtime, &id, inputs)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let _ = recipes::record_recent(&*state.runtime.store, &id).await;
    Ok(Json(report))
}

/// POST /recipes/{id}/render — show-as-code disclosure renders the
/// assembled TOML for the user's current inputs. Read-only; takes
/// the same payload as `/apply` but doesn't materialise.
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

#[derive(Deserialize)]
pub struct ForkBody {
    pub new_name: String,
}

/// POST /recipes/{id}/fork — duplicate into the user library.
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
        Err((StatusCode::NOT_FOUND, format!("user recipe '{id}' not found")))
    }
}

/// GET /recipes/{id}/export — TOML for sharing.
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
