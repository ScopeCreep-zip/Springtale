//! Rule operations — create, delete, toggle, list, run, update.
//!
//! Runtime operations take `&RuntimeState` (need engine).
//! Store operations take `&dyn StorageBackend` (CLI uses these).
//! All three apps call these same functions. Zero duplication.

mod create;
mod execute;
mod query;
mod schema;

use serde::Serialize;

use specta::Type;
pub use create::{
    CreateConnectorRuleRequest, create_connector_rule, create_rule, delete_rule, toggle_rule,
    update_rule,
};
pub use execute::{
    ConnectorTestResult, RunResult, build_synthetic_trigger, reassign_rule_connector, run_rule,
    run_rule_standalone, test_connector,
};
pub use query::{
    add_rule_to_store, delete_rule_from_store, list_rules, list_rules_for_connector,
    list_rules_from_store, toggle_rule_in_store,
};
pub use schema::get_rule_schema;

use crate::error::OperationError;
use crate::state::RuntimeState;

/// Maximum rules per instance (DoS prevention).
const MAX_RULES: usize = 10_000;

/// Rule summary for listing.
#[derive(Debug, Serialize, Type)]
pub struct RuleSummary {
    pub id: String,
    pub name: String,
    pub status: String,
    pub trigger_type: String,
    /// Connector name from trigger (if ConnectorEvent trigger).
    pub connector_name: Option<String>,
    /// Activation error from trigger attach (if any).
    pub activation_error: Option<String>,
}

/// Parse natural language intent into a structured Rule via AI adapter.
///
/// Returns the generated Rule in Disabled status (user reviews before enabling).
/// Does NOT persist — caller previews, then calls `create_rule()` to save.
///
/// Uses the adapter's `parse_rule()` method which builds a prompt from the
/// installed connectors' trigger/action metadata, sends to the LLM, and
/// parses the structured response into a typed `Rule`.
pub async fn parse_rule_from_intent(
    state: &RuntimeState,
    intent: &str,
) -> Result<springtale_core::rule::types::Rule, OperationError> {
    // Build ConnectorInfo list from registry (respecting DataDisclosure).
    // Each connector's triggers and actions are included so the AI knows
    // what's available to compose into rules.
    let registry = state.registry.read().await;
    let available: Vec<springtale_ai::ConnectorInfo> = registry
        .list()
        .iter()
        .filter_map(|(name, _)| {
            let entry = registry.get(name)?;
            let manifest = entry.host.manifest();
            Some(springtale_ai::ConnectorInfo {
                name: manifest.name.clone(),
                description: manifest.description.clone(),
                triggers: manifest
                    .triggers
                    .iter()
                    .map(|t| springtale_ai::adapter::TriggerInfo {
                        name: t.name.clone(),
                        description: t.description.clone(),
                        schema: t.schema.clone(),
                    })
                    .collect(),
                actions: manifest
                    .actions
                    .iter()
                    .map(|a| springtale_ai::adapter::ActionInfo {
                        name: a.name.clone(),
                        description: a.description.clone(),
                        input_schema: a.input_schema.clone(),
                        output_schema: a.output_schema.clone(),
                    })
                    .collect(),
                disclosure_level: springtale_ai::DisclosureLevel::NamesAndDescriptions,
            })
        })
        .collect();
    drop(registry);

    // Call AI adapter's parse_rule method
    let adapter = state.ai_adapter.load();
    let rule = adapter
        .parse_rule(intent, &available)
        .await
        .map_err(|e| OperationError::Ai(format!("{e}")))?;

    tracing::info!(
        rule_name = %rule.name,
        trigger = ?rule.trigger,
        actions = rule.actions.len(),
        "AI generated rule from intent"
    );

    Ok(rule)
}
