//! W2.B — Recipe authoring: save / fork / build-from-scratch.
//!
//! Three pathways into the user library, all click-driven, all
//! sharing the same fundamental flow:
//!
//!   1. Pick / fork / build a `Recipe` shape.
//!   2. Run the W2.C preview as a "Clear Check" (Mario-Maker-style
//!      gate). Save is blocked if preview surfaces errors.
//!   3. Persist to the user-recipes config-store entry. (We keep
//!      the user library in the config store rather than a dedicated
//!      SQL table because every user-facing key/value Springtale
//!      stores today rides the encrypted `config_store` —
//!      `recipes:favorites`, `recipes:recent`, `memory:encryption_key`,
//!      `ai:global`, etc. Adding another key fits the existing
//!      encrypted-at-rest envelope.)
//!
//! Field visibility is **author-declared** per
//! [`super::types::InputField::visibility`] — never inferred from a
//! heuristic. The earlier `auto_classify` helper was deleted in favour
//! of the IFTTT / Zapier / GitHub Actions pattern: the author marks
//! each field `Required | Optional | Advanced | Baked` and the form
//! renders accordingly. Save / Fork / Import accept the recipe as-is.

use serde_json::Value;

use springtale_store::StorageBackend;
use uuid::Uuid;

use crate::error::OperationError;

use super::preview_via_clear_check;
use super::types::{FieldKind, Recipe, RecipeInputs, RecipeSource};

#[derive(Debug, thiserror::Error)]
pub enum AuthoringError {
    #[error("recipe did not pass Clear Check: {0}")]
    ClearCheckFailed(String),
    #[error("recipe id collision with built-in: {0}")]
    BuiltinCollision(String),
    #[error("invalid TOML payload: {0}")]
    InvalidToml(String),
    #[error("operation failed: {0}")]
    Operation(#[from] OperationError),
}

/// Save a recipe to the user library. Runs preview as a Clear Check
/// first; any error blocks the save with the offending error list.
///
/// `recipe` arrives with the author's chosen id, name, description,
/// category, and final `required_inputs` / `optional_inputs` /
/// `advanced_inputs` lists. We force `source = RecipeSource::User`
/// regardless of what the caller passed.
pub async fn save_user_recipe(
    store: &dyn StorageBackend,
    mut recipe: Recipe,
) -> Result<Recipe, AuthoringError> {
    // Reject id collisions with built-ins.
    if super::builtin::get(&recipe.id).is_some() {
        return Err(AuthoringError::BuiltinCollision(recipe.id.clone()));
    }
    // Force the source so a malicious caller can't masquerade as
    // builtin / community.
    recipe.source = RecipeSource::User;

    // Clear Check: synthesise empty + default inputs, run preview.
    let inputs = author_test_inputs(&recipe);
    let report = preview_via_clear_check(&recipe, &inputs);
    if !report.passed {
        return Err(AuthoringError::ClearCheckFailed(report.errors.join("; ")));
    }

    persist(store, &recipe).await?;
    Ok(recipe)
}

/// Fork an existing recipe — duplicates it into the user library
/// under a fresh id, marks `source = User`. The author can then
/// edit it via [`save_user_recipe`] (which overwrites the entry).
pub async fn fork_recipe(
    store: &dyn StorageBackend,
    original: &Recipe,
    new_name: String,
) -> Result<Recipe, AuthoringError> {
    let mut forked = original.clone();
    forked.id = format!("user-{}", Uuid::new_v4());
    forked.name = new_name;
    forked.source = RecipeSource::User;
    persist(store, &forked).await?;
    Ok(forked)
}

/// Delete a user recipe by id. No-op when the id isn't a user recipe.
pub async fn delete_user_recipe(
    store: &dyn StorageBackend,
    recipe_id: &str,
) -> Result<bool, AuthoringError> {
    let mut list = load_all(store).await?;
    let before = list.len();
    list.retain(|r| r.id != recipe_id);
    if list.len() == before {
        return Ok(false);
    }
    write_all(store, &list).await?;
    Ok(true)
}

/// Read every user recipe back. Used by `library::load_user_recipes`
/// so the standard list/filter path surfaces user recipes alongside
/// built-ins.
pub async fn load_user_recipes(
    store: &dyn StorageBackend,
) -> Result<Vec<Recipe>, OperationError> {
    load_all(store).await
}

/// Export a single user recipe as canonical TOML for sharing via
/// file / Signal / Matrix / etc. The TOML round-trips back through
/// [`import_user_recipe_toml`] without lossy conversion.
pub fn export_recipe_toml(recipe: &Recipe) -> Result<String, AuthoringError> {
    toml::to_string_pretty(&recipe).map_err(|e| AuthoringError::InvalidToml(e.to_string()))
}

/// Import a recipe from a TOML blob. The import path runs the same
/// Clear Check as `save_user_recipe`, so a malformed shared recipe
/// never lands in the library.
pub async fn import_recipe_toml(
    store: &dyn StorageBackend,
    toml_payload: &str,
) -> Result<Recipe, AuthoringError> {
    let mut recipe: Recipe =
        toml::from_str(toml_payload).map_err(|e| AuthoringError::InvalidToml(e.to_string()))?;
    // Generate a fresh id so an imported recipe doesn't collide with
    // an existing user-library entry of the same name.
    recipe.id = format!("user-{}", Uuid::new_v4());
    save_user_recipe(store, recipe).await
}

// ── Persistence helpers ───────────────────────────────────────

const RECIPES_USER_KEY: &str = "recipes:user";

async fn load_all(store: &dyn StorageBackend) -> Result<Vec<Recipe>, OperationError> {
    let raw = crate::operations::config::get_config(store, RECIPES_USER_KEY).await?;
    match raw {
        Value::Array(items) => Ok(items
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect()),
        _ => Ok(Vec::new()),
    }
}

async fn write_all(
    store: &dyn StorageBackend,
    recipes: &[Recipe],
) -> Result<(), OperationError> {
    let value = serde_json::to_value(recipes).unwrap_or(Value::Array(Vec::new()));
    crate::operations::config::set_config(store, RECIPES_USER_KEY, value).await
}

async fn persist(
    store: &dyn StorageBackend,
    recipe: &Recipe,
) -> Result<(), OperationError> {
    let mut list = load_all(store).await?;
    list.retain(|r| r.id != recipe.id); // overwrite-by-id semantics
    list.push(recipe.clone());
    write_all(store, &list).await
}

/// Build a synthetic inputs map for the Clear Check that fills in
/// non-empty placeholders for every declared input. The Clear Check
/// needs to actually run the rule TOML through the parser; that
/// parser fails if `${placeholder}` substitutions leave un-quoted
/// gaps in TOML. Using sentinel values catches *real* errors
/// (missing inputs declared but never used, typos) without
/// erroring on "you didn't fill the form yet."
fn author_test_inputs(recipe: &Recipe) -> RecipeInputs {
    let mut inputs = RecipeInputs::empty();
    for f in &recipe.inputs {
        let value = match &f.kind {
            FieldKind::Bool => serde_json::json!(false),
            FieldKind::Number => serde_json::json!(0),
            FieldKind::Url => serde_json::json!("https://example.invalid/"),
            FieldKind::Select { options } => options
                .first()
                .map(|o| serde_json::json!(o.value))
                .unwrap_or(serde_json::json!("")),
            // Use the recipe's default if present; otherwise a
            // sentinel string the Clear Check renders into TOML.
            _ => f
                .default
                .clone()
                .unwrap_or_else(|| serde_json::json!("__author_check__")),
        };
        inputs.insert(f.id.clone(), value);
    }
    inputs
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::operations::recipes::types::{
        Difficulty, RecipeBlueprint, RecipeCategory, RuleStep,
    };

    fn minimal_recipe() -> Recipe {
        Recipe {
            id: "user-test".into(),
            name: "Test".into(),
            description: "".into(),
            icon_id: "".into(),
            category: RecipeCategory::Custom,
            tags: vec![],
            connectors_used: vec![],
            ai_required: false,
            difficulty: Difficulty::Quick,
            source: RecipeSource::User,
            inputs: vec![],
            blueprint: RecipeBlueprint {
                connector_configs: vec![],
                rules: vec![RuleStep {
                    toml: r#"name = "minimal"

[trigger]
type = "Cron"
expression = "0 9 * * *"

[[actions]]
type = "SendMessage"
text = "hello"
"#
                    .into(),
                }],
                ai_config: None,
                summary: None,
            },
        }
    }

    #[tokio::test]
    async fn save_round_trips_through_in_memory_store() {
        let store: std::sync::Arc<dyn StorageBackend> =
            std::sync::Arc::new(springtale_store::SqliteBackend::open_in_memory().unwrap());
        let recipe = minimal_recipe();
        let saved = save_user_recipe(&*store, recipe.clone()).await.unwrap();
        assert_eq!(saved.id, recipe.id);
        let list = load_all(&*store).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, recipe.id);
    }

    #[tokio::test]
    async fn save_rejects_builtin_collision() {
        let store: std::sync::Arc<dyn StorageBackend> =
            std::sync::Arc::new(springtale_store::SqliteBackend::open_in_memory().unwrap());
        let mut recipe = minimal_recipe();
        recipe.id = "telegram-echo".into(); // built-in id
        let result = save_user_recipe(&*store, recipe).await;
        assert!(matches!(result, Err(AuthoringError::BuiltinCollision(_))));
    }

    #[tokio::test]
    async fn fork_clones_with_fresh_id() {
        let store: std::sync::Arc<dyn StorageBackend> =
            std::sync::Arc::new(springtale_store::SqliteBackend::open_in_memory().unwrap());
        let original = super::super::builtin::get("telegram-echo").unwrap();
        let forked = fork_recipe(&*store, &original, "My Echo".into()).await.unwrap();
        assert_ne!(forked.id, original.id);
        assert_eq!(forked.name, "My Echo");
        assert!(matches!(forked.source, RecipeSource::User));
    }

    #[tokio::test]
    async fn toml_round_trip_preserves_recipe() {
        let store: std::sync::Arc<dyn StorageBackend> =
            std::sync::Arc::new(springtale_store::SqliteBackend::open_in_memory().unwrap());
        let recipe = minimal_recipe();
        let toml_str = export_recipe_toml(&recipe).unwrap();
        let imported = import_recipe_toml(&*store, &toml_str).await.unwrap();
        // Id is fresh, but everything else round-trips.
        assert_eq!(imported.name, recipe.name);
        assert_eq!(imported.blueprint.rules.len(), 1);
    }

    #[tokio::test]
    async fn delete_returns_false_when_missing() {
        let store: std::sync::Arc<dyn StorageBackend> =
            std::sync::Arc::new(springtale_store::SqliteBackend::open_in_memory().unwrap());
        let removed = delete_user_recipe(&*store, "nope").await.unwrap();
        assert!(!removed);
    }

    #[tokio::test]
    async fn delete_removes_existing_recipe() {
        let store: std::sync::Arc<dyn StorageBackend> =
            std::sync::Arc::new(springtale_store::SqliteBackend::open_in_memory().unwrap());
        let recipe = minimal_recipe();
        save_user_recipe(&*store, recipe.clone()).await.unwrap();
        let removed = delete_user_recipe(&*store, &recipe.id).await.unwrap();
        assert!(removed);
        let list = load_all(&*store).await.unwrap();
        assert!(list.is_empty());
    }
}
