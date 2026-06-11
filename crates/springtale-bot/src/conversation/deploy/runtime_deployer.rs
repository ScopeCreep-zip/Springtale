//! `RecipeDeployer` backed by a live `RuntimeState`.
//!
//! Constructed by the host (daemon / desktop) with the `RuntimeState`
//! and `EmbeddedScheduler` it owns, then injected into the bot via
//! `BotBuilder::recipe_deployer`. Both runtime types are `Clone` (cheap
//! — all `Arc`s), so the deployer holds owned copies for the bot's
//! lifetime.
//!
//! `deploy` mirrors the exact sequence the existing recipe-apply command
//! uses (`apply_recipe` THEN activate every created rule's triggers) —
//! `apply_recipe` deliberately leaves activation to the caller, so
//! without the second step a deployed recipe would sit in the store +
//! engine but never fire. Activation = schedule cron/filewatch AND
//! attach ConnectorEvent handlers (the shared `activate_rule`): a
//! connector-event rule only in the `RuleEngine` never fires until a
//! handler is subscribed on the connector host, so chat-deployed
//! messaging bots need the attach exactly like cron bots need the
//! schedule.

use async_trait::async_trait;

use springtale_runtime::EmbeddedScheduler;
use springtale_runtime::RuntimeState;
use springtale_runtime::operations::preflight::preflight_recipe;
use springtale_runtime::operations::preflight::types::PreflightReport;
use springtale_runtime::operations::recipes::types::{ApplyReport, RecipeInputs};
use springtale_runtime::operations::recipes::{apply_recipe, record_recent};

use super::trait_::{DeployError, RecipeDeployer};

/// Deploys recipes against a live runtime and schedules their triggers.
pub struct RuntimeRecipeDeployer {
    runtime: RuntimeState,
    scheduler: EmbeddedScheduler,
}

impl RuntimeRecipeDeployer {
    pub fn new(runtime: RuntimeState, scheduler: EmbeddedScheduler) -> Self {
        Self { runtime, scheduler }
    }
}

#[async_trait]
impl RecipeDeployer for RuntimeRecipeDeployer {
    async fn preflight(
        &self,
        recipe_id: &str,
        inputs: &RecipeInputs,
    ) -> Result<PreflightReport, DeployError> {
        preflight_recipe(&self.runtime, recipe_id, inputs.clone())
            .await
            .map_err(|e| DeployError::Failed(e.to_string()))
    }

    async fn deploy(
        &self,
        recipe_id: &str,
        inputs: RecipeInputs,
    ) -> Result<ApplyReport, DeployError> {
        let report = apply_recipe(&self.runtime, recipe_id, inputs)
            .await
            .map_err(|e| DeployError::Failed(e.to_string()))?;

        // Activate every freshly-created rule's triggers — schedule
        // cron/filewatch AND attach ConnectorEvent handlers — through the
        // shared registry on RuntimeState (set at boot). Without the
        // attach, a chat-deployed messaging recipe would sit in the engine
        // and never fire.
        let rules = self
            .runtime
            .store
            .list_rules()
            .await
            .map_err(|e| DeployError::Failed(format!("listing rules after apply: {e}")))?;
        let registry = self.runtime.trigger_registry.get();
        for rule_id in &report.rules_created {
            if let Some(rule) = rules.iter().find(|r| r.id.0.to_string() == *rule_id) {
                match registry {
                    Some(registry) => {
                        springtale_runtime::activate_rule(
                            rule,
                            &self.scheduler,
                            registry,
                            &self.runtime.registry,
                        )
                        .await;
                    }
                    // Before bootstrap (shouldn't happen for chat deploys,
                    // which run post-boot): at least schedule cron/filewatch.
                    None => {
                        if let Err(e) = self.scheduler.schedule(rule).await {
                            tracing::warn!(
                                rule = %rule.name,
                                error = %e,
                                "failed to schedule trigger for chat-deployed rule"
                            );
                        }
                    }
                }
            }
        }

        // Reflect the deploy in the recently-used list, like the UI path.
        let _ = record_recent(&*self.runtime.store, recipe_id).await;

        Ok(report)
    }
}
