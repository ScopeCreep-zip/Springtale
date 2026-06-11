//! Within-fire chain state — what the dispatcher threads between
//! actions in a `Chain`.
//!
//! Today (pre-Phase-0) the dispatcher returned `Result<String, String>`
//! and discarded step outputs between iterations. Recipes referenced
//! `${last_ai_output}` / `${last_connector_output}` expecting some
//! state to flow, but the placeholders never resolved at runtime — ~12
//! shipped recipes literally posted `${last_ai_output}` to users.
//!
//! `ChainContext` is the typed envelope that closes the gap. The
//! dispatcher mutates it per step: builds a [`StepOutput`], appends to
//! `steps`, refreshes the relevant `last_*_output` alias. Subsequent
//! steps read via the runtime-state template grammar in
//! [`super::template_resolve`].
//!
//! ## Scope
//!
//! `ChainContext` is **within-fire only** — discarded after the chain
//! returns. Cross-fire state (dedupe, executions, bot memory) lives in
//! `springtale-store` tables, not here. The boundary line is explicit:
//! a recipe author who needs "remember the last 5 fetched values" uses
//! a separate primitive (planned for Phase B+).
//!
//! ## Type policy
//!
//! No `specta::Type` derive — `ChainContext` is recursive via
//! `StepOutput::output: serde_json::Value` and is never a typed Tauri
//! command parameter. The dispatcher returns it as part of an opaque
//! execution-result envelope; the executions log surfaces flat
//! projections (rows with byte-sizes, status) to the UI, not the raw
//! payload (per privacy invariants in CLAUDE.md §6).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// State threaded through one chain fire.
///
/// Constructed by the rule dispatcher when a [`super::Rule`] fires;
/// mutated per [`super::Action`] step; consumed by the executions
/// logger after the chain returns.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChainContext {
    /// Original trigger payload — referenced by recipes as
    /// `${trigger.path.to.field}`. Shape varies by trigger kind:
    /// webhooks pass the parsed HTTP body, connector events pass the
    /// connector-emitted JSON, cron passes `Value::Null` (no payload).
    pub trigger: serde_json::Value,

    /// Steps executed so far, in chain order. 1-indexed in template
    /// references (`${step1.field}` = first step). Make.com convention,
    /// chosen over n8n's by-name addressing because most Springtale
    /// recipes don't name their steps — positional is the bimbo-mode
    /// default; names are opt-in.
    pub steps: Vec<StepOutput>,

    /// Most-recent text output of an [`super::Action::AiComplete`]
    /// step. Recipes use `${last_ai_output}` as a shorthand. `None`
    /// until the first AiComplete step runs.
    pub last_ai_output: Option<String>,

    /// Most-recent structured output of an
    /// [`super::Action::RunConnector`] step. Holds the connector's
    /// raw `ActionResult.output` JSON so recipe authors can write
    /// natural paths like `${last_connector_output.status}` for an
    /// HTTP `get` step (which returns `{ status, headers, body }`)
    /// without wrapper indirection. The human-readable connector
    /// message lives separately on
    /// [`Self::last_connector_message`].
    pub last_connector_output: Option<serde_json::Value>,

    /// Most-recent human-readable `ActionResult.message` from a
    /// [`super::Action::RunConnector`] step. Surfaced to chat-command
    /// handlers and the executions log so the connector's
    /// human-facing line ("Posted to #general", "OK 200") is
    /// accessible without parsing the raw `output` JSON.
    pub last_connector_message: Option<String>,

    /// Most-recent output of an [`super::Action::Extract`] step
    /// (Phase A). Schema depends on the [`super::ExtractKind`] —
    /// Readability returns `{ title, content, ... }`, Css returns the
    /// author's schema map, Feed returns `{ entries: [...] }`, etc.
    pub last_extract_output: Option<serde_json::Value>,

    /// Most-recent output of an [`super::Action::Dedupe`] step.
    /// Shape: `{ "outcome": "fresh" | "seen_before", "key": "..." }`.
    pub last_dedupe_output: Option<serde_json::Value>,

    /// Most-recent output of an [`super::Action::RunShell`] step.
    /// Shape today: `{ command, executed: false, reason: "ShellExec
    /// capability gate" }` — the dispatcher logs the command but
    /// does not execute it without an approval flow. Real stdout
    /// arrives when the ShellExec approval path lands (Phase 1+).
    /// Aliased so recipes referencing `${last_shell_output}` resolve
    /// cleanly today.
    pub last_shell_output: Option<serde_json::Value>,
}

impl ChainContext {
    /// Construct a fresh context for a chain fire, seeded with the
    /// trigger payload.
    pub fn new(trigger: serde_json::Value) -> Self {
        Self {
            trigger,
            steps: Vec::new(),
            last_ai_output: None,
            last_connector_output: None,
            last_connector_message: None,
            last_extract_output: None,
            last_dedupe_output: None,
            last_shell_output: None,
        }
    }

    /// Record a completed step and refresh the matching `last_*_output`
    /// alias. The dispatcher calls this after each successful action.
    pub fn record_step(&mut self, step: StepOutput) {
        match step.kind.as_str() {
            "ai_complete" => {
                if let Some(text) = step.output.get("text").and_then(|v| v.as_str()) {
                    self.last_ai_output = Some(text.to_owned());
                }
            }
            "run_connector" => {
                // StepOutput.output for run_connector has shape
                // `{ output: Value, message: String, success: bool }`.
                // Split into the recipe-natural alias (raw output)
                // plus the human-message sibling.
                if let Some(out) = step.output.get("output") {
                    self.last_connector_output = Some(out.clone());
                }
                if let Some(msg) = step.output.get("message").and_then(|m| m.as_str()) {
                    self.last_connector_message = Some(msg.to_owned());
                }
            }
            "extract" => {
                self.last_extract_output = Some(step.output.clone());
            }
            "dedupe" => {
                self.last_dedupe_output = Some(step.output.clone());
            }
            "run_shell" => {
                self.last_shell_output = Some(step.output.clone());
            }
            _ => {}
        }
        self.steps.push(step);
    }

    /// Next 1-indexed step number. Used by the dispatcher when
    /// constructing a fresh [`StepOutput`].
    pub fn next_step_index(&self) -> usize {
        self.steps.len() + 1
    }

    /// One-line summary callers use in `tracing::info!` after a
    /// chain completes — gives a stable shape (step count + last
    /// step's kind) without leaking full payloads to logs (privacy
    /// invariant per CLAUDE.md §6.10).
    pub fn brief(&self) -> String {
        match self.steps.last() {
            Some(last) => format!(
                "chain: {} step(s), last={} ({}ms)",
                self.steps.len(),
                last.kind,
                last.duration_ms
            ),
            None => "chain: 0 steps".to_owned(),
        }
    }

    /// Look up a step by author-given name. Returns the first match
    /// (duplicate names are rejected at preflight, see
    /// [`super::template_resolve::validate_step_names`]).
    pub fn step_by_name(&self, name: &str) -> Option<&StepOutput> {
        self.steps.iter().find(|s| s.name.as_deref() == Some(name))
    }
}

/// Result of one dispatched [`super::Action`] inside a chain.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StepOutput {
    /// Position in the chain (1-indexed).
    pub index: usize,
    /// Action discriminant tag — `"run_connector"` | `"send_message"`
    /// | `"write_file"` | `"run_shell"` | `"notify"` | `"chain"` |
    /// `"transform"` | `"delay"` | `"ai_complete"` | `"extract"` |
    /// `"dedupe"`. The dispatcher sets this from the matched
    /// [`super::Action`] variant.
    pub kind: String,
    /// Optional author-given name, used for `${step.NAME.field}`
    /// references. Authors don't have to name steps; if they do, the
    /// names must be unique (preflight-enforced).
    #[serde(default)]
    pub name: Option<String>,
    /// Structured output produced by the step. Always JSON; AiComplete
    /// wraps text as `{"text": "..."}` so the path grammar stays
    /// uniform.
    pub output: serde_json::Value,
    /// Wall-clock duration in milliseconds. Useful for the executions
    /// log and per-step "Test this step" preview.
    pub duration_ms: u64,
    /// Soft-failure message — set when the step failed but the chain
    /// continued (e.g. dedupe early-termination, an optional sub-step
    /// declared `continue_on_error`).
    #[serde(default)]
    pub error: Option<String>,
}

/// Failure modes for chain execution. Distinct from
/// [`crate::error::CoreError`] because chain failures carry rich step
/// context the dispatcher reports through the executions log.
#[derive(Debug, thiserror::Error)]
pub enum ChainError {
    /// Template referenced `${stepN.*}` for an N that hasn't run yet.
    #[error("step {0} referenced before it ran")]
    StepNotYetRun(usize),

    /// Template referenced `${step.NAME.*}` for a name with no match.
    #[error("step name `{0}` not found in chain")]
    StepNameNotFound(String),

    /// Two steps in the chain share a `name`. Preflight invariant.
    #[error("duplicate step name in chain: `{0}`")]
    DuplicateStepName(String),

    /// A [`super::Action::Dedupe`] step found the key already seen —
    /// the chain runner catches this and ends cleanly with execution
    /// status `empty` (not a failure).
    #[error("chain suppressed by dedupe")]
    Suppressed,

    /// Underlying action step failed.
    #[error("step {index} ({kind}) failed: {message}")]
    StepFailed {
        index: usize,
        kind: String,
        message: String,
    },

    /// Chain nesting exceeded [`super::action::MAX_CHAIN_DEPTH`].
    #[error("chain depth {depth} exceeds max {max}")]
    DepthExceeded { depth: u32, max: u32 },

    /// Template grammar error not covered by the resolver.
    #[error("template resolution failed: {0}")]
    Template(String),
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ai_step(idx: usize, text: &str) -> StepOutput {
        StepOutput {
            index: idx,
            kind: "ai_complete".into(),
            name: None,
            output: json!({ "text": text }),
            duration_ms: 12,
            error: None,
        }
    }

    fn connector_step(idx: usize, body: serde_json::Value) -> StepOutput {
        // Mirrors the dispatcher's run_connector wrapper shape: the
        // raw connector output is nested under `output`, with
        // `success` / `message` siblings. `record_step` unwraps to
        // populate `last_connector_output` (raw) +
        // `last_connector_message`.
        StepOutput {
            index: idx,
            kind: "run_connector".into(),
            name: None,
            output: serde_json::json!({
                "success": true,
                "message": "ok",
                "output": body,
            }),
            duration_ms: 34,
            error: None,
        }
    }

    #[test]
    fn new_context_has_empty_state() {
        let c = ChainContext::new(json!({ "x": 1 }));
        assert_eq!(c.trigger["x"], 1);
        assert!(c.steps.is_empty());
        assert!(c.last_ai_output.is_none());
        assert!(c.last_connector_output.is_none());
        assert!(c.last_extract_output.is_none());
        assert!(c.last_dedupe_output.is_none());
    }

    #[test]
    fn next_step_index_is_1_based() {
        let mut c = ChainContext::new(json!(null));
        assert_eq!(c.next_step_index(), 1);
        c.record_step(ai_step(1, "hi"));
        assert_eq!(c.next_step_index(), 2);
    }

    #[test]
    fn record_ai_step_refreshes_last_ai_output() {
        let mut c = ChainContext::new(json!(null));
        c.record_step(ai_step(1, "first"));
        assert_eq!(c.last_ai_output.as_deref(), Some("first"));
        c.record_step(ai_step(2, "second"));
        assert_eq!(c.last_ai_output.as_deref(), Some("second"));
    }

    #[test]
    fn record_connector_step_refreshes_last_connector_output() {
        let mut c = ChainContext::new(json!(null));
        c.record_step(connector_step(1, json!({ "body": "hello" })));
        assert_eq!(c.last_connector_output.as_ref().unwrap()["body"], "hello");
    }

    #[test]
    fn record_extract_step_refreshes_last_extract_output() {
        let mut c = ChainContext::new(json!(null));
        let step = StepOutput {
            index: 1,
            kind: "extract".into(),
            name: None,
            output: json!({ "title": "page title" }),
            duration_ms: 7,
            error: None,
        };
        c.record_step(step);
        assert_eq!(
            c.last_extract_output.as_ref().unwrap()["title"],
            "page title"
        );
    }

    #[test]
    fn record_dedupe_step_refreshes_last_dedupe_output() {
        let mut c = ChainContext::new(json!(null));
        let step = StepOutput {
            index: 1,
            kind: "dedupe".into(),
            name: None,
            output: json!({ "outcome": "fresh", "key": "abc" }),
            duration_ms: 1,
            error: None,
        };
        c.record_step(step);
        assert_eq!(c.last_dedupe_output.as_ref().unwrap()["outcome"], "fresh");
    }

    #[test]
    fn step_by_name_returns_first_match() {
        let mut c = ChainContext::new(json!(null));
        let mut step = ai_step(1, "summary");
        step.name = Some("summarise".into());
        c.record_step(step);
        let found = c.step_by_name("summarise").expect("step missing");
        assert_eq!(found.output["text"], "summary");
        assert!(c.step_by_name("nope").is_none());
    }

    #[test]
    fn round_trips_through_json() {
        let mut c = ChainContext::new(json!({ "trig": true }));
        c.record_step(ai_step(1, "ok"));
        c.record_step(connector_step(2, json!({ "status": 200 })));
        let s = serde_json::to_string(&c).unwrap();
        let back: ChainContext = serde_json::from_str(&s).unwrap();
        assert_eq!(back.steps.len(), 2);
        assert_eq!(back.last_ai_output.as_deref(), Some("ok"));
        assert_eq!(back.last_connector_output.as_ref().unwrap()["status"], 200);
        assert_eq!(back.last_connector_message.as_deref(), Some("ok"));
    }

    #[test]
    fn record_connector_step_splits_output_and_message() {
        let mut c = ChainContext::new(json!(null));
        c.record_step(connector_step(1, json!({ "body": "hello", "status": 200 })));
        // last_connector_output exposes the raw connector output so
        // recipes can write `${last_connector_output.status}`
        // without wrapper indirection.
        assert_eq!(c.last_connector_output.as_ref().unwrap()["status"], 200);
        assert_eq!(c.last_connector_output.as_ref().unwrap()["body"], "hello");
        // last_connector_message holds the human line.
        assert_eq!(c.last_connector_message.as_deref(), Some("ok"));
    }
}
