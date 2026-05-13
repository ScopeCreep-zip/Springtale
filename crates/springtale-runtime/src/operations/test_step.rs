//! W2.C / Phase C — single-step recipe test (n8n "Execute Node" pattern).
//!
//! "Test This Step" is the deploy-form button per step in the
//! frontend's RecipeDeployPanel. It fires the chain up to and
//! including the targeted step in DryRun mode, then returns the
//! last [`StepOutput`] for the UI to render.
//!
//! Why dispatch up to N rather than just N: producing realistic
//! output for step N usually requires the upstream
//! `last_*_output` aliases populated by steps 1..N-1. The simplest
//! correct path is to run the chain in DryRun mode (side-effecting
//! arms stubbed, read arms real) and stop after step N.
//!
//! ## Privacy + safety
//!
//! - DryRun mode never writes to dedupe, never sends messages,
//!   never writes files. Read-only connector actions (HTTP get,
//!   browser navigate / get_html / extract, AiComplete, Extract,
//!   Transform, Delay) run for real so the user sees realistic
//!   upstream payloads.
//! - The result is **not** persisted to the executions log under
//!   the rule's normal stream — the dispatcher records the run
//!   with `mode = "dry_run"` so it's distinguishable, and the
//!   default 14-day retention sweeps it like any other row.
//!
//! ## Pinned upstream (future)
//!
//! v1 always re-fires the chain from step 1. n8n's pinned-data
//! pattern (skip steps 1..N-1, seed last_* aliases from cached
//! values) is a Phase C+ enhancement; today's UX is "click Test
//! This Step on step 1, then 2, then 3" — the user walks forward.

use serde::{Deserialize, Serialize};
use specta::Type;

use springtale_core::rule::types::Rule;
use springtale_core::rule::{ChainError, StepOutput, Trigger};
use springtale_cooperation::execution::{ExecutionContext, ExecutionMode};

use crate::operations::recipes::apply::ApplyError;
use crate::operations::recipes::library;
use crate::operations::recipes::types::RecipeInputs;
use crate::state::RuntimeState;

/// Failure modes for [`test_recipe_step`].
#[derive(Debug, thiserror::Error)]
pub enum TestStepError {
    #[error(transparent)]
    Apply(#[from] ApplyError),
    #[error("rule index {requested} out of range (recipe has {available} rules)")]
    RuleIndexOutOfRange { requested: usize, available: usize },
    #[error("step index {requested} out of range (rule has {available} actions)")]
    StepIndexOutOfRange { requested: usize, available: usize },
    #[error("dispatch failed: {0}")]
    Dispatch(String),
}

/// Result of one test-step run.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct TestStepReport {
    pub recipe_id: String,
    pub rule_index: usize,
    pub step_index: usize,
    /// `true` when the dispatcher returned `Ok` and the targeted
    /// step was reached. `false` when the chain failed or
    /// short-circuited before the step.
    pub ran: bool,
    /// The targeted step's recorded output. Populated only when
    /// `ran == true`. Schema mirrors the dispatcher's
    /// [`StepOutput`].
    pub step: Option<TestStepOutput>,
    /// Every step the dispatcher recorded en route — useful for
    /// the UI's "see the upstream" disclosure. Steps prior to the
    /// targeted one ran in DryRun mode (read-only arms real,
    /// side-effecting arms stubbed).
    pub upstream: Vec<TestStepOutput>,
    /// Set when the chain failed. Mirrors the executions-log
    /// `error_kind` taxonomy.
    pub error: Option<String>,
}

/// IPC-shaped projection of [`StepOutput`]. Flat for specta;
/// `output` is a serialized JSON value rather than a recursive
/// type (matches the executions log pattern).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct TestStepOutput {
    pub index: usize,
    pub kind: String,
    pub name: Option<String>,
    /// JSON output rendered as a string so specta can describe it
    /// without recursing into `serde_json::Value` (per
    /// `feedback_specta_recursive_types`).
    pub output_json: String,
    pub duration_ms: u64,
    pub error: Option<String>,
}

impl From<&StepOutput> for TestStepOutput {
    fn from(s: &StepOutput) -> Self {
        Self {
            index: s.index,
            kind: s.kind.clone(),
            name: s.name.clone(),
            output_json: s.output.to_string(),
            duration_ms: s.duration_ms,
            error: s.error.clone(),
        }
    }
}

/// Fire one step of one rule of `recipe_id` against `inputs`. The
/// dispatcher runs in DryRun mode through `step_index`; the
/// returned report carries the targeted step's recorded output.
pub async fn test_recipe_step(
    state: &RuntimeState,
    recipe_id: &str,
    inputs: RecipeInputs,
    rule_index: usize,
    step_index: usize,
) -> Result<TestStepReport, TestStepError> {
    let recipe = library::get_recipe(&*state.store, recipe_id)
        .await
        .map_err(|e| TestStepError::Apply(ApplyError::Operation(e)))?
        .ok_or_else(|| TestStepError::Apply(ApplyError::RecipeNotFound(recipe_id.to_owned())))?;

    let rule_count = recipe.blueprint.rules.len();
    let rule_step = recipe.blueprint.rules.get(rule_index).ok_or(
        TestStepError::RuleIndexOutOfRange {
            requested: rule_index,
            available: rule_count,
        },
    )?;

    let resolved_toml =
        crate::operations::recipes::apply::substitute_template_public(&rule_step.toml, &inputs);
    let rule: Rule = toml::from_str(&resolved_toml).map_err(|e| {
        TestStepError::Apply(ApplyError::InvalidRuleToml(e.to_string()))
    })?;

    let actions = rule.actions.as_slice();
    if step_index >= actions.len() {
        return Err(TestStepError::StepIndexOutOfRange {
            requested: step_index,
            available: actions.len(),
        });
    }

    // Slice the action list at [0..=step_index]. The dispatcher
    // runs every action through to the target step inclusive.
    let to_run = &actions[..=step_index];

    let trigger_payload = synthetic_trigger_payload(&rule.trigger);

    let execution =
        ExecutionContext::for_global(rule.id, ExecutionMode::DryRun);

    let sentinel = state.sentinel.clone();
    let chain_result = crate::dispatch::dispatch_actions(
        to_run,
        &state.capability_bridge,
        &sentinel,
        execution,
        trigger_payload,
    )
    .await;

    let (ran, step, upstream, error) = match chain_result {
        Ok(chain) => {
            let targeted = chain.steps.last().map(TestStepOutput::from);
            let upstream: Vec<TestStepOutput> = chain
                .steps
                .iter()
                .map(TestStepOutput::from)
                .collect();
            (true, targeted, upstream, None)
        }
        Err(ChainError::Suppressed) => (
            false,
            None,
            Vec::new(),
            Some("chain suppressed by dedupe (would short-circuit production run)".into()),
        ),
        Err(e) => (false, None, Vec::new(), Some(e.to_string())),
    };

    Ok(TestStepReport {
        recipe_id: recipe_id.to_owned(),
        rule_index,
        step_index,
        ran,
        step,
        upstream,
        error,
    })
}

/// Build the trigger payload `dispatch_actions` carries as the
/// chain's `${trigger.*}` reference family. Derived directly from
/// the rule's [`Trigger`] so recipes that reference
/// `${trigger.path}` / `${trigger.payload}` (webhook-fanout-multi,
/// cross-channel-broadcast, etc.) get a realistic stand-in during
/// Test This Step.
fn synthetic_trigger_payload(trigger: &Trigger) -> serde_json::Value {
    match trigger {
        Trigger::Cron { expression } => serde_json::json!({
            "trigger_kind": "cron",
            "expression": expression,
            "manual_trigger": true,
        }),
        Trigger::FileWatch { path, event } => serde_json::json!({
            "trigger_kind": "file_watch",
            "path": path,
            "event": event,
            "manual_trigger": true,
        }),
        Trigger::Webhook { path } => serde_json::json!({
            "trigger_kind": "webhook",
            "path": path,
            "payload": {},
            "manual_trigger": true,
        }),
        Trigger::ConnectorEvent { connector, event } => serde_json::json!({
            "trigger_kind": "connector_event",
            "connector": connector,
            "event": event,
            "payload": {},
            "manual_trigger": true,
        }),
        Trigger::SystemEvent { event } => serde_json::json!({
            "trigger_kind": "system_event",
            "event": event,
            "payload": {},
            "manual_trigger": true,
        }),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_step_output_from_step_output_preserves_fields() {
        let step = StepOutput {
            index: 3,
            kind: "send_message".into(),
            name: Some("greeting".into()),
            output: json!({ "text": "hi", "dry_run": true }),
            duration_ms: 42,
            error: None,
        };
        let ipc: TestStepOutput = (&step).into();
        assert_eq!(ipc.index, 3);
        assert_eq!(ipc.kind, "send_message");
        assert_eq!(ipc.name.as_deref(), Some("greeting"));
        assert_eq!(ipc.duration_ms, 42);
        assert!(ipc.output_json.contains("\"text\":\"hi\""));
        assert!(ipc.output_json.contains("\"dry_run\":true"));
    }

    #[test]
    fn synthetic_trigger_payload_carries_trigger_kind() {
        let trigger = Trigger::Cron {
            expression: "0 7 * * *".into(),
        };
        let payload = synthetic_trigger_payload(&trigger);
        assert_eq!(payload["trigger_kind"], "cron");
        assert_eq!(payload["expression"], "0 7 * * *");
        assert_eq!(payload["manual_trigger"], true);
    }

    #[test]
    fn synthetic_trigger_payload_for_webhook_has_payload_object() {
        let trigger = Trigger::Webhook {
            path: "/inbox".into(),
        };
        let payload = synthetic_trigger_payload(&trigger);
        assert_eq!(payload["trigger_kind"], "webhook");
        assert_eq!(payload["path"], "/inbox");
        assert!(payload["payload"].is_object());
    }
}
