//! The deploy port the conversation engine calls to materialize a
//! recipe.
//!
//! `apply_recipe`/`preflight_recipe` live in `springtale-runtime` and
//! require a `&RuntimeState`, which the embedded `Bot` does not hold.
//! Rather than thread runtime state into the bot, the bot calls this
//! trait; the host (daemon / desktop) implements it with the
//! `RuntimeState` it owns AND replicates the host-side scheduling step
//! (`scheduler.schedule(rule)`) that `apply_recipe` deliberately leaves
//! to the caller — without it a deployed cron recipe would never fire.

use std::sync::Arc;

use async_trait::async_trait;

use springtale_runtime::operations::preflight::types::PreflightReport;
use springtale_runtime::operations::recipes::types::{ApplyReport, RecipeInputs};

/// Failure modes surfaced to the user as a conversational apology.
#[derive(Debug, thiserror::Error)]
pub enum DeployError {
    #[error("{0}")]
    Failed(String),
}

/// Implemented by the host that owns the `RuntimeState`.
#[async_trait]
pub trait RecipeDeployer: Send + Sync {
    /// Validate the gathered inputs against the recipe (required fields,
    /// connector availability, schedule sanity) before deploying.
    async fn preflight(
        &self,
        recipe_id: &str,
        inputs: &RecipeInputs,
    ) -> Result<PreflightReport, DeployError>;

    /// Apply the recipe AND register any created cron/file-watch rules
    /// with the scheduler so they actually run.
    async fn deploy(
        &self,
        recipe_id: &str,
        inputs: RecipeInputs,
    ) -> Result<ApplyReport, DeployError>;
}

/// Shared, optional handle held by the `Bot`. `None` in headless/test
/// contexts — the engine degrades to a graceful "can't deploy here".
pub type SharedDeployer = Arc<dyn RecipeDeployer>;
