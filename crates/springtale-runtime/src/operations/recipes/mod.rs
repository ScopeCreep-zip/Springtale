//! Recipes — click-and-play starting points for building bots and teams.
//!
//! See `types.rs` for the data model; `builtin.rs` for the starter
//! library compiled into the binary; `library.rs` for server-side
//! search/filter/sort and favorites/recent persistence. Future
//! modules: `apply.rs` (run a recipe against the live runtime),
//! `authoring.rs` (W2.B field-classification heuristics),
//! `clear_check.rs` (Mario-Maker-style "must run before save"),
//! `export_import.rs` (TOML round-trip).
//!
//! Architecture invariant — see the plan's "Architecture invariant"
//! section: every recipe decision (which fields are required vs
//! optional vs advanced, which categories exist, what counts as a
//! match for a query) lives here. Frontends render what they're
//! told.

pub mod apply;
pub mod authoring;
pub mod builtin;
pub mod library;
pub mod pieces;
pub mod types;

pub use apply::{apply_recipe, render_blueprint_toml, ApplyError};
pub use authoring::{
    delete_user_recipe, export_recipe_toml, fork_recipe, import_recipe_toml,
    load_user_recipes, save_user_recipe, AuthoringError,
};
pub use library::{
    get_recipe, list_categories, list_recipes, record_recent, toggle_favorite,
};
pub use pieces::{list_pieces, RecipePiece, RecipePieceSummary};

/// Re-export of [`crate::operations::preview::preview_blueprint`] under
/// the name the W2.B authoring Clear Check expects. Keeps the call
/// site in `authoring.rs` from depending on the broader preview
/// module surface.
pub fn preview_via_clear_check(
    recipe: &types::Recipe,
    inputs: &types::RecipeInputs,
) -> crate::operations::preview::PreviewReport {
    crate::operations::preview::preview_blueprint(recipe, inputs)
}
pub use types::{
    ApplyReport, ConnectorConfigStep, Difficulty, FieldKind, FieldVisibility, InputField,
    Recipe, RecipeBlueprint, RecipeCategory, RecipeFilter, RecipeInputs, RecipeSort,
    RecipeSource, RecipeSourceFilter, RuleStep, SelectOption,
};
