//! W2.D — Modular piece composition.
//!
//! Lets a power user borrow named slots from one recipe and reuse
//! them in a from-scratch team. Pokemon-Showdown's "import box"
//! pattern: the user picks "Use just the trigger from the GitHub
//! Watcher recipe" rather than the whole recipe.
//!
//! For W2.D the supported piece kinds map 1:1 to the
//! [`RecipeBlueprint`] fields:
//!
//! - `RuleStep` — rule TOML
//! - `ConnectorConfigStep` — connector config seed
//! - `AiConfigStep` — AI provider config
//!
//! All slots are returned with placeholders still embedded — the
//! consuming UI is responsible for re-substituting any inputs the
//! user supplies. That keeps the surface dumb and reusable.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::OperationError;
use crate::state::RuntimeState;

use super::library;
use super::types::{AiConfigStep, ConnectorConfigStep, Recipe, RuleStep};

/// One slot the caller can borrow from a recipe.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecipePiece {
    Trigger { rule: RuleStep },
    ConnectorConfig { step: ConnectorConfigStep },
    AiConfig { step: AiConfigStep },
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RecipePieceSummary {
    /// `"trigger:0"`, `"connector_config:1"`, etc. — stable id the
    /// frontend uses as a list key + to request the full piece.
    pub id: String,
    /// One-line label rendered in the picker dropdown.
    pub label: String,
    pub piece: RecipePiece,
}

/// Enumerate every piece a recipe exposes for borrowing. Pure —
/// reads only the recipe definition, no runtime state changes.
pub async fn list_pieces(
    state: &RuntimeState,
    recipe_id: &str,
) -> Result<Vec<RecipePieceSummary>, OperationError> {
    let recipe = library::get_recipe(&*state.store, recipe_id).await?;
    let Some(recipe) = recipe else {
        return Ok(Vec::new());
    };
    Ok(extract_pieces(&recipe))
}

fn extract_pieces(recipe: &Recipe) -> Vec<RecipePieceSummary> {
    let mut out = Vec::new();
    for (idx, rule) in recipe.blueprint.rules.iter().enumerate() {
        // Best-effort label: pull the `[rule] name = "..."` line if
        // present, otherwise fall back to the index.
        let label = rule_name_from_toml(&rule.toml)
            .map(|n| format!("Rule: {n}"))
            .unwrap_or_else(|| format!("Rule #{idx} from {}", recipe.name));
        out.push(RecipePieceSummary {
            id: format!("trigger:{idx}"),
            label,
            piece: RecipePiece::Trigger { rule: rule.clone() },
        });
    }
    for (idx, step) in recipe.blueprint.connector_configs.iter().enumerate() {
        out.push(RecipePieceSummary {
            id: format!("connector_config:{idx}"),
            label: format!("Connector config: {}", step.connector_name),
            piece: RecipePiece::ConnectorConfig { step: step.clone() },
        });
    }
    if let Some(ai) = &recipe.blueprint.ai_config {
        out.push(RecipePieceSummary {
            id: "ai_config:0".into(),
            label: format!("AI config ({})", ai.target),
            piece: RecipePiece::AiConfig { step: ai.clone() },
        });
    }
    out
}

fn rule_name_from_toml(toml: &str) -> Option<String> {
    // Very small parser: scan for `name = "..."` inside the first
    // `[rule]` block. Avoids a full toml parse for what is purely a
    // label.
    let mut in_rule = false;
    for line in toml.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("[rule]") {
            in_rule = true;
            continue;
        }
        if trimmed.starts_with('[') {
            in_rule = false;
        }
        if in_rule && let Some(rest) = trimmed.strip_prefix("name") {
            let after_eq = rest.split_once('=').map(|(_, v)| v.trim())?;
            let stripped = after_eq.trim_matches('"').trim_matches('\'');
            if !stripped.is_empty() {
                return Some(stripped.to_owned());
            }
        }
    }
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::operations::recipes::builtin;

    #[test]
    fn telegram_echo_exposes_trigger_and_connector_pieces() {
        let recipe = builtin::get("telegram-echo").unwrap();
        let pieces = extract_pieces(&recipe);
        // Telegram echo has 1 rule + 1 connector config.
        assert!(
            pieces
                .iter()
                .any(|p| matches!(p.piece, RecipePiece::Trigger { .. }))
        );
        assert!(
            pieces
                .iter()
                .any(|p| matches!(p.piece, RecipePiece::ConnectorConfig { .. }))
        );
    }

    #[test]
    fn rule_name_extracted_from_toml() {
        let toml = r#"[rule]
name = "telegram-echo"

[trigger]
type = "ConnectorEvent"
"#;
        assert_eq!(rule_name_from_toml(toml).as_deref(), Some("telegram-echo"));
    }

    #[test]
    fn llm_assistant_exposes_ai_piece() {
        let recipe = builtin::get("llm-assistant").unwrap();
        let pieces = extract_pieces(&recipe);
        assert!(
            pieces
                .iter()
                .any(|p| matches!(p.piece, RecipePiece::AiConfig { .. }))
        );
    }
}
