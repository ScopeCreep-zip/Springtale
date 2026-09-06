//! W2.C — Live preview (dry-run) for a recipe.
//!
//! Renders what a recipe would do without actually deploying it.
//! Substitutes the user's inputs into each rule's TOML, parses each
//! rule, fires `build_synthetic_trigger` per rule, evaluates against
//! an in-memory `RuleEngine`, and returns plain-language narrative
//! steps the frontend renders as a comic strip.
//!
//! No side effects — preview never touches the live runtime's
//! engine, store, or registry. The engine is constructed fresh and
//! dropped when the function returns. This makes preview safe to
//! call from anywhere (recipe authoring Clear Check, deploy form
//! "Preview" button, etc.).
//!
//! Action dispatch is *not* simulated — we surface the action
//! discriminant + params verbatim so the user sees what would run.
//! Wiring a mock-connector dispatcher is a follow-up; for the
//! click-and-play UX, "would run X with Y" is enough.

use serde::{Deserialize, Serialize};
use specta::Type;

use springtale_core::rule::engine::RuleEngine;
use springtale_core::rule::types::Rule;

use super::recipes::apply::ApplyError;
use super::recipes::library;
use super::recipes::types::{Recipe, RecipeInputs};

/// One step in the comic-strip narrative.
#[derive(Debug, Clone, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub struct PreviewStep {
    /// Who is "speaking" (rule name, connector name, or "system").
    pub speaker: String,
    /// Plain-language description of what happens at this step.
    pub narrative: String,
    /// What target (chat id, file path, etc.) the bot would have
    /// dispatched to. `None` when the step is informational.
    pub would_send_to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub struct PreviewReport {
    pub recipe_id: String,
    pub steps: Vec<PreviewStep>,
    /// `false` when at least one rule failed to parse or evaluate.
    /// The frontend uses this to gate the recipe-authoring "Clear
    /// Check" (W2.B); deploy form just renders the steps regardless.
    pub passed: bool,
    /// Errors encountered during preview (parse failure, missing
    /// placeholder, etc.). Empty when `passed`.
    pub errors: Vec<String>,
}

/// Dry-run a recipe.
pub async fn preview_recipe(
    state: &crate::state::RuntimeState,
    recipe_id: &str,
    inputs: RecipeInputs,
) -> Result<PreviewReport, ApplyError> {
    let recipe = library::get_recipe(&*state.store, recipe_id)
        .await?
        .ok_or_else(|| ApplyError::RecipeNotFound(recipe_id.to_owned()))?;
    Ok(preview_blueprint(&recipe, &inputs))
}

/// Pure version — operates on a `Recipe` + inputs, no runtime state.
/// Used by the W2.B authoring Clear Check before the recipe is
/// saved (it doesn't exist in the library yet).
pub fn preview_blueprint(recipe: &Recipe, inputs: &RecipeInputs) -> PreviewReport {
    let mut steps = Vec::new();
    let mut errors = Vec::new();

    // Always lead with the recipe's own summary if it has one.
    if let Some(summary) = recipe.blueprint.summary.as_ref() {
        steps.push(PreviewStep {
            speaker: recipe.name.clone(),
            narrative: summary.clone(),
            would_send_to: None,
        });
    }

    // Connector config steps — show one bubble per connector.
    for cfg in &recipe.blueprint.connector_configs {
        let resolved = super::recipes::apply::substitute_value_public(&cfg.config, inputs);
        steps.push(PreviewStep {
            speaker: "system".into(),
            narrative: format!(
                "Configure {} with {}.",
                cfg.connector_name,
                redact_secrets(&resolved)
            ),
            would_send_to: None,
        });
    }

    // Rule steps — parse, evaluate against synthetic trigger.
    for rule_step in &recipe.blueprint.rules {
        let toml = super::recipes::apply::substitute_template_public(&rule_step.toml, inputs);
        let rule: Rule = match toml::from_str(&toml) {
            Ok(r) => r,
            Err(e) => {
                errors.push(format!("rule TOML parse error: {e}"));
                continue;
            }
        };
        let trigger_label = trigger_label(&rule);
        steps.push(PreviewStep {
            speaker: rule.name.clone(),
            narrative: format!("Trigger: {trigger_label}"),
            would_send_to: None,
        });

        let synthetic = crate::operations::rules::build_synthetic_trigger(&rule);
        let mut engine = RuleEngine::new();
        let _ = engine.add_rule(rule.clone());
        let matches = engine.evaluate(&synthetic);
        for m in matches {
            for action in m.actions.iter() {
                let (narrative, would_send_to) = describe_action(action);
                steps.push(PreviewStep {
                    speaker: rule.name.clone(),
                    narrative,
                    would_send_to,
                });
            }
        }
    }

    // AI config step.
    if let Some(ai) = &recipe.blueprint.ai_config {
        steps.push(PreviewStep {
            speaker: "system".into(),
            narrative: format!("Configure AI ({}).", ai.target.key()),
            would_send_to: None,
        });
    }

    let passed = errors.is_empty();
    PreviewReport {
        recipe_id: recipe.id.clone(),
        steps,
        passed,
        errors,
    }
}

fn trigger_label(rule: &Rule) -> String {
    use springtale_core::rule::Trigger;
    match &rule.trigger {
        Trigger::Cron { expression } => format!("cron `{expression}`"),
        Trigger::FileWatch { path, event } => format!("file {path} ({event})"),
        Trigger::Webhook { path } => format!("webhook {path}"),
        Trigger::ConnectorEvent { connector, event } => {
            format!("{connector} event `{event}`")
        }
        Trigger::SystemEvent { event } => format!("system event `{event}`"),
    }
}

fn describe_action(action: &springtale_core::rule::Action) -> (String, Option<String>) {
    use springtale_core::rule::Action;
    match action {
        Action::SendMessage { text } => (format!("Send message: {text}"), None),
        Action::RunConnector {
            connector,
            action: act,
            params,
        } => {
            let target = params
                .get("chat_id")
                .or_else(|| params.get("to"))
                .or_else(|| params.get("repo"))
                .map(|v| v.to_string());
            (
                format!(
                    "Run {connector}.{act} with {}",
                    redact_secrets(&serde_json::Value::Object(params.clone()))
                ),
                target,
            )
        }
        Action::WriteFile {
            destination,
            content,
            delete_source,
        } => {
            let op = if *delete_source { "Move" } else { "Write" };
            (
                format!("{op} to {destination}: {content}"),
                Some(destination.clone()),
            )
        }
        Action::RunShell { command } => (format!("Run shell: {command}"), None),
        Action::Notify { title, body } => (format!("Notify: {title} — {body}"), None),
        Action::Chain { steps } => (format!("Chain ({} steps)", steps.len()), None),
        Action::Transform { operation, params } => (
            format!(
                "Transform {operation} with {}",
                redact_secrets(&serde_json::Value::Object(params.clone()))
            ),
            None,
        ),
        Action::Delay { seconds } => (format!("Pause for {seconds}s"), None),
        Action::AiComplete { prompt, adapter } => {
            let adapter_label = adapter.as_deref().unwrap_or("default");
            (format!("Ask AI ({adapter_label}): {prompt}"), None)
        }
        Action::Extract { source, kind } => {
            let kind_label = match kind {
                springtale_core::rule::action::ExtractKind::Readability => "readability",
                springtale_core::rule::action::ExtractKind::Css { .. } => "css",
                springtale_core::rule::action::ExtractKind::JsonPath { .. } => "json_path",
                springtale_core::rule::action::ExtractKind::LlmSchema { .. } => "llm_schema",
                springtale_core::rule::action::ExtractKind::Feed => "feed",
                springtale_core::rule::action::ExtractKind::Ical { .. } => "ical",
                springtale_core::rule::action::ExtractKind::Passthrough => "passthrough",
                springtale_core::rule::action::ExtractKind::PageDiff => "page_diff",
            };
            (format!("Extract `{kind_label}` from {source}"), None)
        }
        Action::Dedupe {
            key,
            bucket,
            history,
        } => (
            format!("Dedupe by `{key}` in bucket `{bucket}` (history {history})"),
            None,
        ),
    }
}

fn redact_secrets(value: &serde_json::Value) -> String {
    use serde_json::Value;
    fn walk(v: &Value) -> Value {
        match v {
            Value::Object(map) => {
                let mut out = serde_json::Map::new();
                for (k, val) in map {
                    let lower = k.to_lowercase();
                    if lower.contains("token")
                        || lower.contains("secret")
                        || lower.contains("password")
                        || lower.contains("api_key")
                    {
                        out.insert(k.clone(), Value::String("***".into()));
                    } else {
                        out.insert(k.clone(), walk(val));
                    }
                }
                Value::Object(out)
            }
            Value::Array(arr) => Value::Array(arr.iter().map(walk).collect()),
            other => other.clone(),
        }
    }
    serde_json::to_string(&walk(value)).unwrap_or_default()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::operations::recipes::builtin;
    use serde_json::json;

    #[test]
    fn telegram_echo_preview_passes_with_token() {
        let recipe = builtin::get("telegram-echo").unwrap();
        let mut inputs = RecipeInputs::empty();
        inputs.insert("bot_token", json!("12345:abc"));
        inputs.insert("reply_prefix", json!(""));
        let report = preview_blueprint(&recipe, &inputs);
        assert!(report.passed, "errors: {:?}", report.errors);
        assert!(!report.steps.is_empty());
    }

    #[test]
    fn redacted_secrets_dont_appear_in_narrative() {
        let recipe = builtin::get("telegram-echo").unwrap();
        let mut inputs = RecipeInputs::empty();
        inputs.insert("bot_token", json!("super-secret-token"));
        inputs.insert("reply_prefix", json!(""));
        let report = preview_blueprint(&recipe, &inputs);
        let joined = report
            .steps
            .iter()
            .map(|s| s.narrative.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!joined.contains("super-secret-token"));
    }
}
