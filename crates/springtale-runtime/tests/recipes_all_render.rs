//! Every builtin recipe renders with placeholder inputs, every rendered rule
//! parses, and every rendered `RunConnector` step satisfies the first-party
//! connector's action `input_schema` (plan 5.3, finding 90).
//!
//! Pure render path only — no `RuntimeState`, no network. Connector
//! manifests come from the `inventory` factories; `ConnectorFactory` exposes
//! no manifest, so only connectors that instantiate without config
//! are checked through their factory manifests.

use std::collections::HashMap;

use serde_json::{Value, json};
use springtale_connector::FactoryEntry;
use springtale_core::rule::types::Rule;
use springtale_runtime::operations::recipes::action_schema::{ActionOutcome, check_rule_actions};
use springtale_runtime::operations::recipes::apply::substitute_template_public;
use springtale_runtime::operations::recipes::builtin;
use springtale_runtime::operations::recipes::types::{FieldKind, InputField, RecipeInputs};

/// The field's default, else a type-appropriate placeholder.
fn placeholder(field: &InputField) -> Value {
    if let Some(default) = &field.default {
        return default.clone();
    }
    match &field.kind {
        FieldKind::Number => json!(1),
        FieldKind::Bool => json!(true),
        FieldKind::Url => json!("https://example.com"),
        FieldKind::Cron => json!("0 * * * *"),
        FieldKind::Select { options } => options
            .first()
            .map(|o| json!(o.value))
            .unwrap_or_else(|| json!("example")),
        FieldKind::JsonSchema { example } => example
            .clone()
            .unwrap_or_else(|| json!({ "type": "object" })),
        _ => json!("example"),
    }
}

fn all_manifests() -> HashMap<String, springtale_connector::manifest::ConnectorManifest> {
    // Every factory exposes its static manifest (plan finding 121), so no
    // connector needs credentials to be validated against.
    inventory::iter::<FactoryEntry>
        .into_iter()
        .map(|entry| (entry.factory.name().to_owned(), entry.factory.manifest()))
        .collect()
}

#[tokio::test]
async fn test_every_builtin_recipe_renders_and_validates_action_params() {
    let manifests = all_manifests();
    assert!(!manifests.is_empty(), "no first-party factories registered");

    let (mut validated, mut skipped) = (0usize, 0usize);
    let mut failures = Vec::new();
    let recipes = builtin::all();
    assert!(!recipes.is_empty());

    for recipe in &recipes {
        let mut inputs = RecipeInputs::empty();
        for field in &recipe.inputs {
            inputs.insert(field.id.clone(), placeholder(field));
        }
        for (idx, step) in recipe.blueprint.rules.iter().enumerate() {
            let toml = substitute_template_public(&step.toml, &inputs);
            let rule: Rule = match toml::from_str(&toml) {
                Ok(rule) => rule,
                Err(e) => {
                    failures.push(format!("{} rule[{idx}]: TOML: {e}", recipe.id));
                    continue;
                }
            };
            let checks = check_rule_actions(&rule, |name| {
                manifests.get(name).map(|m| m.actions.as_slice())
            });
            for check in checks {
                match check.outcome {
                    ActionOutcome::Skipped => skipped += 1,
                    ActionOutcome::Valid => validated += 1,
                    ActionOutcome::Invalid(reason) => failures.push(format!(
                        "{} rule[{idx}] {}: {reason}",
                        recipe.id, check.step
                    )),
                }
            }
        }
    }

    println!(
        "{} recipes: validated {validated} RunConnector steps, skipped {skipped} \
         (connector not among the first-party factories)",
        recipes.len()
    );
    assert!(
        failures.is_empty(),
        "{} recipe steps failed validation:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
