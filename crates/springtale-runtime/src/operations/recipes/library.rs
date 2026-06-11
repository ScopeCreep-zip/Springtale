//! Recipe library — server-side listing, filtering, and sorting.
//!
//! All search / filter / sort logic lives here so every frontend
//! (desktop IPC, dashboard HTTP, CLI, future surfaces) shares the
//! same results. Frontends never hold the full catalogue and never
//! filter in-memory — they pass a [`RecipeFilter`] and render the
//! response.
//!
//! User recipes (created via W2.B authoring) will be merged into the
//! same listing once the `recipes_user` SQLite table lands. The
//! current implementation reads built-ins only; the merge point is
//! [`load_user_recipes`] (returns `Ok(Vec::new())` until the table
//! exists).

use springtale_store::StorageBackend;

use crate::error::OperationError;

use super::builtin;
use super::types::{
    Recipe, RecipeCategory, RecipeFilter, RecipeSort, RecipeSource, RecipeSourceFilter,
};

const MAX_RESULTS: usize = 100;

/// List recipes matching the supplied filter.
///
/// Order:
///   1. `sources` filter (e.g. only built-ins) — applied first because
///      it's a cheap discriminant.
///   2. `category` filter.
///   3. `tags` filter (AND across tags).
///   4. `favorites_only` (consults the config store).
///   5. `query` fuzzy match against name + description + connectors_used.
///   6. Sort (`Recommended` / `Name` / `Recent`).
///   7. Truncate to `limit` (capped at [`MAX_RESULTS`]).
pub async fn list_recipes(
    store: &dyn StorageBackend,
    filter: RecipeFilter,
) -> Result<Vec<Recipe>, OperationError> {
    let mut all: Vec<Recipe> = builtin::all();
    all.extend(load_user_recipes(store).await?);

    let favorites = if filter.favorites_only {
        load_favorites(store).await?
    } else {
        Vec::new()
    };

    let q = filter.query.as_deref().map(str::to_lowercase);

    let matches: Vec<Recipe> = all
        .into_iter()
        .filter(|r| matches_sources(r, &filter.sources))
        .filter(|r| filter.category.is_none_or(|c| r.category == c))
        .filter(|r| filter.tags.iter().all(|t| r.tags.iter().any(|rt| rt == t)))
        .filter(|r| {
            if !filter.favorites_only {
                return true;
            }
            favorites.iter().any(|id| id == &r.id)
        })
        .filter(|r| q.as_ref().is_none_or(|q| recipe_matches_query(r, q)))
        .collect();

    let mut sorted = matches;
    sort_recipes(&mut sorted, filter.sort, store).await?;

    let cap = filter.limit.unwrap_or(MAX_RESULTS).min(MAX_RESULTS);
    sorted.truncate(cap);
    Ok(sorted)
}

/// Fetch a single recipe by id (built-in or user).
pub async fn get_recipe(
    store: &dyn StorageBackend,
    id: &str,
) -> Result<Option<Recipe>, OperationError> {
    if let Some(r) = builtin::get(id) {
        return Ok(Some(r));
    }
    let user = load_user_recipes(store).await?;
    Ok(user.into_iter().find(|r| r.id == id))
}

/// Read the user-recipes config-store entry. W2.B authoring persists
/// each saved recipe through `recipes:user`; this loads them back so
/// they appear in the standard list/filter path alongside built-ins.
async fn load_user_recipes(store: &dyn StorageBackend) -> Result<Vec<Recipe>, OperationError> {
    super::authoring::load_user_recipes(store).await
}

/// Read the favorites list from the config store (`recipes:favorites`
/// stores a JSON array of recipe ids).
async fn load_favorites(store: &dyn StorageBackend) -> Result<Vec<String>, OperationError> {
    let raw = crate::operations::config::get_config(store, "recipes:favorites").await?;
    match raw {
        serde_json::Value::Array(items) => Ok(items
            .into_iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect()),
        _ => Ok(Vec::new()),
    }
}

/// Toggle a recipe's favorite status. Returns the new state (`true` =
/// now a favorite). Persists via the config store so the choice is
/// the same across desktop / dashboard / CLI.
pub async fn toggle_favorite(
    store: &dyn StorageBackend,
    recipe_id: &str,
) -> Result<bool, OperationError> {
    let mut favs = load_favorites(store).await?;
    let now_favorite = if let Some(pos) = favs.iter().position(|id| id == recipe_id) {
        favs.remove(pos);
        false
    } else {
        favs.push(recipe_id.to_owned());
        true
    };
    let value = serde_json::Value::Array(favs.into_iter().map(serde_json::Value::String).collect());
    crate::operations::config::set_config(store, "recipes:favorites", value).await?;
    Ok(now_favorite)
}

/// Push a recipe id onto the "recently used" list (capped at 12).
pub async fn record_recent(
    store: &dyn StorageBackend,
    recipe_id: &str,
) -> Result<(), OperationError> {
    let raw = crate::operations::config::get_config(store, "recipes:recent").await?;
    let mut recent: Vec<String> = match raw {
        serde_json::Value::Array(items) => items
            .into_iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    };
    recent.retain(|id| id != recipe_id);
    recent.insert(0, recipe_id.to_owned());
    recent.truncate(12);
    let value =
        serde_json::Value::Array(recent.into_iter().map(serde_json::Value::String).collect());
    crate::operations::config::set_config(store, "recipes:recent", value).await?;
    Ok(())
}

/// Return the list of available categories. Frontend renders the
/// sidebar from this — never hardcodes the category list.
pub fn list_categories() -> Vec<RecipeCategory> {
    vec![
        RecipeCategory::Messaging,
        RecipeCategory::Coding,
        RecipeCategory::Web,
        RecipeCategory::AiAssistant,
        RecipeCategory::Daily,
        RecipeCategory::SafetyPrivacy,
        RecipeCategory::Custom,
    ]
}

fn matches_sources(recipe: &Recipe, filters: &[RecipeSourceFilter]) -> bool {
    if filters.is_empty() {
        return true;
    }
    let recipe_kind = match recipe.source {
        RecipeSource::Builtin => RecipeSourceFilter::Builtin,
        RecipeSource::User => RecipeSourceFilter::User,
        RecipeSource::Community { .. } => RecipeSourceFilter::Community,
    };
    filters.contains(&recipe_kind)
}

fn recipe_matches_query(r: &Recipe, q: &str) -> bool {
    r.name.to_lowercase().contains(q)
        || r.description.to_lowercase().contains(q)
        || r.connectors_used
            .iter()
            .any(|c| c.to_lowercase().contains(q))
        || r.tags.iter().any(|t| t.to_lowercase().contains(q))
}

async fn sort_recipes(
    recipes: &mut [Recipe],
    sort: RecipeSort,
    store: &dyn StorageBackend,
) -> Result<(), OperationError> {
    match sort {
        RecipeSort::Recommended => {
            recipes.sort_by(|a, b| {
                // Built-ins first, then by Difficulty (Quick < Standard < Power), then by name.
                source_rank(&a.source)
                    .cmp(&source_rank(&b.source))
                    .then_with(|| difficulty_rank(a.difficulty).cmp(&difficulty_rank(b.difficulty)))
                    .then_with(|| a.name.cmp(&b.name))
            });
        }
        RecipeSort::Name => {
            recipes.sort_by(|a, b| a.name.cmp(&b.name));
        }
        RecipeSort::Recent => {
            let raw = crate::operations::config::get_config(store, "recipes:recent").await?;
            let recent: Vec<String> = match raw {
                serde_json::Value::Array(items) => items
                    .into_iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect(),
                _ => Vec::new(),
            };
            let index = |id: &str| recent.iter().position(|r| r == id).unwrap_or(usize::MAX);
            recipes.sort_by(|a, b| {
                index(&a.id)
                    .cmp(&index(&b.id))
                    .then_with(|| a.name.cmp(&b.name))
            });
        }
    }
    Ok(())
}

fn source_rank(s: &RecipeSource) -> u8 {
    match s {
        RecipeSource::Builtin => 0,
        RecipeSource::User => 1,
        RecipeSource::Community { .. } => 2,
    }
}

fn difficulty_rank(d: super::types::Difficulty) -> u8 {
    use super::types::Difficulty::*;
    match d {
        Quick => 0,
        Standard => 1,
        Power => 2,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::operations::recipes::types::Difficulty;
    use springtale_store::SqliteBackend;
    use std::sync::Arc;

    fn test_store() -> Arc<dyn StorageBackend> {
        Arc::new(SqliteBackend::open_in_memory().unwrap())
    }

    #[tokio::test]
    async fn list_returns_all_builtins_by_default() {
        let store = test_store();
        let recipes = list_recipes(&*store, RecipeFilter::default())
            .await
            .unwrap();
        assert!(
            recipes.len() >= 50,
            "expected ≥50 built-in recipes, got {}",
            recipes.len()
        );
    }

    #[tokio::test]
    async fn category_filter_narrows_results() {
        let store = test_store();
        let filter = RecipeFilter {
            category: Some(RecipeCategory::Messaging),
            ..Default::default()
        };
        let recipes = list_recipes(&*store, filter).await.unwrap();
        assert!(!recipes.is_empty());
        for r in recipes {
            assert_eq!(r.category, RecipeCategory::Messaging);
        }
    }

    #[tokio::test]
    async fn query_filter_is_fuzzy_across_name_and_tags() {
        let store = test_store();
        let filter = RecipeFilter {
            query: Some("telegram".into()),
            ..Default::default()
        };
        let recipes = list_recipes(&*store, filter).await.unwrap();
        assert!(!recipes.is_empty());
        for r in &recipes {
            let matches = r.name.to_lowercase().contains("telegram")
                || r.tags.iter().any(|t| t.contains("telegram"))
                || r.connectors_used.iter().any(|c| c.contains("telegram"));
            assert!(
                matches,
                "{} matched query 'telegram' but no field contains it",
                r.name
            );
        }
    }

    #[tokio::test]
    async fn recommended_sort_puts_quick_first() {
        let store = test_store();
        let recipes = list_recipes(&*store, RecipeFilter::default())
            .await
            .unwrap();
        // Find first Power-difficulty index — must be after the last Quick index.
        let last_quick = recipes
            .iter()
            .rposition(|r| matches!(r.difficulty, Difficulty::Quick));
        let first_power = recipes
            .iter()
            .position(|r| matches!(r.difficulty, Difficulty::Power));
        if let (Some(lq), Some(fp)) = (last_quick, first_power) {
            assert!(
                lq < fp,
                "Power-difficulty appeared before a Quick-difficulty in recommended sort"
            );
        }
    }

    #[tokio::test]
    async fn toggle_favorite_round_trips_through_config_store() {
        let store = test_store();
        assert!(toggle_favorite(&*store, "telegram-echo").await.unwrap());
        let favs = load_favorites(&*store).await.unwrap();
        assert_eq!(favs, vec!["telegram-echo".to_string()]);
        assert!(!toggle_favorite(&*store, "telegram-echo").await.unwrap());
        let favs = load_favorites(&*store).await.unwrap();
        assert!(favs.is_empty());
    }

    #[tokio::test]
    async fn favorites_only_filter_respects_state() {
        let store = test_store();
        toggle_favorite(&*store, "telegram-echo").await.unwrap();
        let filter = RecipeFilter {
            favorites_only: true,
            ..Default::default()
        };
        let recipes = list_recipes(&*store, filter).await.unwrap();
        assert_eq!(recipes.len(), 1);
        assert_eq!(recipes[0].id, "telegram-echo");
    }

    #[tokio::test]
    async fn record_recent_caps_at_12() {
        let store = test_store();
        for i in 0..20 {
            record_recent(&*store, &format!("recipe-{i}"))
                .await
                .unwrap();
        }
        let raw = crate::operations::config::get_config(&*store, "recipes:recent")
            .await
            .unwrap();
        let recent = raw.as_array().unwrap();
        assert_eq!(recent.len(), 12);
        assert_eq!(recent[0].as_str().unwrap(), "recipe-19");
    }

    #[test]
    fn list_categories_includes_all_variants() {
        let categories = list_categories();
        assert_eq!(categories.len(), 7);
    }
}
