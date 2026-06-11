//! Preflight engine — orchestrates checks and returns one [`PreflightReport`].
//!
//! All checks live in `checks.rs`. The engine's job is just to call
//! them in a sensible order, collect their [`PreflightItem`]s, and
//! aggregate the report. Server-side decision: the engine determines
//! `deployable` from the items, not the frontend.

use crate::error::OperationError;
use crate::state::RuntimeState;

use super::super::recipes::library;
use super::super::recipes::types::RecipeInputs;
use super::checks;
use super::types::{PreflightItem, PreflightReport};

/// Run preflight for the supplied recipe + user inputs.
pub async fn preflight_recipe(
    state: &RuntimeState,
    recipe_id: &str,
    inputs: RecipeInputs,
) -> Result<PreflightReport, OperationError> {
    let recipe = library::get_recipe(&*state.store, recipe_id).await?;
    let Some(recipe) = recipe else {
        return Ok(PreflightReport {
            recipe_id: recipe_id.to_owned(),
            items: vec![PreflightItem {
                id: "recipe:not_found".into(),
                label: format!("Recipe '{recipe_id}' not found"),
                status: super::types::PreflightStatus::Blocking,
                detail: Some(
                    "The recipe is unknown to this runtime. It may have been deleted or never installed.".into(),
                ),
                fix_hint: None,
            }],
            deployable: false,
            has_warnings: false,
        });
    };

    let mut items = Vec::new();
    // Order matters for readability — required inputs first because
    // they're the most actionable; connector + AI rows last so the
    // user sees "wait, fill the token first" before "configure AI."
    items.extend(checks::check_required_inputs(&recipe, &inputs));
    items.extend(checks::check_optional_format(&recipe, &inputs));
    items.extend(checks::check_schedule_frequency(&recipe, &inputs));
    items.extend(checks::check_connectors(state, &recipe).await);
    if let Some(item) = checks::check_browser_runtime(&recipe) {
        items.push(item);
    }
    if let Some(item) = checks::check_ai_required(state, &recipe).await {
        items.push(item);
    }
    if let Some(item) =
        checks::check_structured_extraction_capability(state, &recipe, &inputs).await
    {
        items.push(item);
    }

    Ok(PreflightReport::from_items(recipe.id.clone(), items))
}

// Engine integration tests are covered by the dedicated checks.rs
// unit tests + the desktop/dashboard end-to-end run (W1.D verification
// scenarios in the plan). Constructing a full RuntimeState in-process
// is heavy and would duplicate the existing init.rs surface.
