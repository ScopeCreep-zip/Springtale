use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Maximum nesting depth for Chain actions.
pub const MAX_CHAIN_DEPTH: u32 = 4;

/// Maximum number of actions per rule (including Chain steps at each level).
/// Prevents job queue flooding from a single trigger event.
pub const MAX_ACTIONS_PER_RULE: usize = 100;

/// An action to perform when a rule's conditions are met.
///
/// Actions are the "do" part of a rule. They execute sequentially within
/// a pipeline. `Chain` allows multi-step workflows.
///
/// Note: `RunConnector` stores the connector name as a String — springtale-core
/// has no dependency on springtale-connector. The dispatch from action to
/// actual connector call happens in the application layer.
///
/// **No `specta::Type` derive (architectural).** The `Chain { steps:
/// Vec<Action> }` variant is self-referential; specta v2.0.0-rc.25's
/// type-graph walker stack-overflows on it during `Builder.export()`.
/// And we don't need Type: no Tauri command has `Action` (or `Rule`,
/// `Condition`, `Trigger`) in its signature — the rule builder posts
/// JSON via `create_rule(rule: serde_json::Value)` and the backend
/// deserializes. The frontend learns the rule shape from
/// `get_rule_schema()`'s JSON Schema (schemars), not specta. This
/// matches Spacedrive's "don't expose internal types over IPC"
/// pattern. Do NOT re-add `Type` here without first introducing a
/// flat wrapper struct.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum Action {
    /// Call a connector action.
    RunConnector {
        /// Connector name (e.g., "connector-kick").
        connector: String,
        /// Action name (e.g., "send_chat").
        action: String,
        /// Parameters passed to the connector action.
        #[serde(default)]
        params: serde_json::Map<String, serde_json::Value>,
    },

    /// Send a message (destination determined by context).
    SendMessage {
        /// Message text (may contain template variables like `${trigger.field}`).
        text: String,
    },

    /// Write to a file.
    WriteFile {
        /// Destination path (may contain template variables).
        destination: String,
        /// Content to write (may contain template variables).
        #[serde(default)]
        content: String,
        /// Whether to delete the source file (for move operations).
        #[serde(default)]
        delete_source: bool,
    },

    /// Execute a shell command (requires ShellExec capability).
    RunShell {
        /// Command to execute.
        command: String,
    },

    /// Send a notification.
    Notify {
        /// Notification title.
        title: String,
        /// Notification body (may contain template variables).
        #[serde(default)]
        body: String,
    },

    /// Execute a sequence of actions as a pipeline.
    Chain {
        /// Ordered list of sub-actions.
        steps: Vec<Action>,
    },

    /// Transform the pipeline data (field extraction, formatting).
    Transform {
        /// Operation: "extract", "format", "filter".
        operation: String,
        /// Operation-specific parameters.
        #[serde(default)]
        params: serde_json::Map<String, serde_json::Value>,
    },

    /// Delay execution for a specified duration.
    Delay {
        /// Delay in seconds.
        seconds: u64,
    },

    /// Call the user's AI adapter (Phase 2a — skipped if NoopAdapter).
    AiComplete {
        /// Prompt text (may contain template variables).
        prompt: String,
        /// Which adapter to use (optional, uses default if omitted).
        #[serde(default)]
        adapter: Option<String>,
    },

    /// Extract structured data from text/HTML/JSON in chain state.
    /// Phase A — replaces `Transform { operation: "extract" }` for the
    /// canonical extraction-ladder cases (Readability / CSS / JSONPath
    /// / LLM schema / Feed / iCal). The dispatcher reads from
    /// `source` (a chain-context reference like
    /// `"last_connector_output.body"`) and writes the extracted JSON
    /// to `last_extract_output`.
    ///
    /// See [`ExtractKind`] for the per-kind input/output schemas.
    Extract {
        /// Chain-context path to read the input from. Resolved via
        /// the runtime-state template grammar:
        /// `"trigger.payload"`, `"last_connector_output.body"`,
        /// `"step3.html"`, etc. The dispatcher resolves this against
        /// the live [`super::ChainContext`] before extraction runs.
        source: String,
        kind: ExtractKind,
    },

    /// Deduplicate the chain by a key derived from chain state.
    /// Phase A — powers the "alert only on new items" pattern in
    /// universal recipes (rss-broadcast, page-change-watcher,
    /// calendar-feed-reminder, etc.).
    ///
    /// The dispatcher resolves `key` and `bucket` against the live
    /// [`super::ChainContext`], hashes the resolved key with blake3
    /// (privacy-safe — no plaintext key on disk), and writes to the
    /// `dedupe_seen` table scoped by
    /// `(formation_id, rule_id, bucket)`. On `DedupeOutcome::SeenBefore`
    /// the dispatcher short-circuits the chain (returns
    /// [`super::ChainError::Suppressed`]) so subsequent action steps
    /// don't fire. Caller marks the execution row `status = 'empty'`
    /// rather than `failed`.
    Dedupe {
        /// Template that resolves to the dedup key. Common shapes:
        /// `"${last_extract_output.entries.0.id}"` (per-feed-entry),
        /// `"${last_extract_output.hash}"` (whole-page diff),
        /// `"${trigger.message_id}"` (per-trigger-event).
        key: String,
        /// Logical bucket — usually the recipe id so multiple polling
        /// recipes on one bot don't collide. Template-substitutable
        /// so authors can split a bucket per channel etc.
        bucket: String,
        /// Max keys retained per `(formation_id, rule_id, bucket)`
        /// before LRU eviction. Default 10,000 follows n8n's
        /// `Remove Duplicates` node default.
        #[serde(default = "default_dedupe_history")]
        history: u32,
    },
}

fn default_dedupe_history() -> u32 {
    10_000
}

/// The extraction layer applied by [`Action::Extract`]. Recipe
/// authors pick the kind that matches the source data:
///
/// - HTML article body → [`ExtractKind::Readability`]
/// - HTML with stable CSS structure → [`ExtractKind::Css`]
/// - JSON API response → [`ExtractKind::JsonPath`]
/// - RSS/Atom/JSON Feed → [`ExtractKind::Feed`]
/// - iCalendar (.ics) → [`ExtractKind::Ical`]
/// - AI-driven schema extraction → [`ExtractKind::LlmSchema`]
/// - Source already typed → [`ExtractKind::Passthrough`]
///
/// No `specta::Type` derive — `Action` doesn't have it either (see
/// the module-level doc comment), and `Css` / `JsonPath` carry
/// arbitrary `serde_json::Map` schemas the rule builder posts as
/// `serde_json::Value`. The frontend renders an extraction-editor
/// keyed off the `kind` discriminator via the JSON Schema in
/// `get_rule_schema()`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExtractKind {
    /// Mozilla Readability — strip nav/ads, return the main article
    /// body. Output: `{ title, byline, content (HTML), text_content,
    /// excerpt }`. Implementation: `dom_smoothie` crate.
    Readability,

    /// CSS-selector schema. Author provides a map
    /// `{ field_name: "css selector" }`; each selector resolves to
    /// the first match's text content. Suffix conventions:
    ///   - `selector :all` — returns an array of all matches' text.
    ///   - `selector @attr` — returns the named attribute instead
    ///     of text (e.g. `"a.link @href"`).
    ///
    /// Implementation: `scraper` crate (html5ever + Servo selectors).
    Css {
        schema: serde_json::Map<String, serde_json::Value>,
    },

    /// JSONPath (RFC 9535) schema. Author provides a map
    /// `{ field_name: "$.json.path[*].selector" }`; each path
    /// resolves via `serde_json_path`. Whole-string paths return
    /// the matched value with its original JSON type preserved.
    JsonPath {
        schema: serde_json::Map<String, serde_json::Value>,
    },

    /// LLM-driven structured extraction. The AI adapter receives
    /// the source text plus the JSON Schema and returns a typed
    /// object. Errors with [`crate::rule::ChainError::StepFailed`]
    /// when the runtime is configured with `NoopAdapter` — the
    /// extraction-ladder fallback to CSS / Readability is the
    /// recipe author's responsibility, not the dispatcher's.
    /// Phase B activates the runtime side.
    LlmSchema {
        /// JSON Schema describing the expected output shape.
        schema: serde_json::Value,
        /// Optional natural-language hint prepended to the LLM
        /// prompt — e.g. `"Extract the product price and
        /// availability."`.
        #[serde(default)]
        hint: Option<String>,
    },

    /// RSS / Atom / JSON Feed parsing via `feed-rs`. Output shape:
    /// `{ entries: [{ id, title, url, published, summary }] }`.
    /// Recipe authors pipe `${last_extract_output.entries.0.url}`
    /// into a dedupe key for "alert on new entries" semantics.
    Feed,

    /// iCalendar parsing (`icalendar` + `rrule`). Output:
    /// `{ events: [{ uid, summary, starts_at, ends_at, location }] }`.
    /// Recurrences are expanded to a window around `now` (default
    /// ±30d) via the optional `window_days` knob; authors set it
    /// when their cron is wider/narrower than the default.
    Ical {
        /// Days before/after `now` to materialize recurring events.
        /// Defaults to 30. Cap at 365 to bound the expansion cost.
        #[serde(default)]
        window_days: Option<i32>,
    },

    /// Pass-through — the source value is already in the shape the
    /// next chain step expects. Useful when an upstream
    /// [`Action::RunConnector`] returns typed JSON and the recipe
    /// just needs to forward it under the
    /// [`super::ChainContext::last_extract_output`] alias for the
    /// dedupe / messaging steps to consume.
    Passthrough,

    /// Page-diff hash. Wraps `extraction::diff::hash` (`ammonia` +
    /// `blake3`) — sanitizes the input HTML / text, normalises
    /// whitespace, returns
    /// `{ hash: "<blake3 hex>", normalised_len: <usize> }`. The
    /// `page-change-watcher` universal recipe uses this with a
    /// dedupe step to fire only when the rendered page actually
    /// changed. Distinct from `Readability` because we don't want
    /// the main-article extraction — we want a stable digest of
    /// the watched region.
    PageDiff,
}

impl Action {
    /// Walk this action and every nested [`Action::Chain`] step, yielding the
    /// leaf actions in execution order. A non-chain action yields itself.
    pub fn iter_leaves(&self) -> Vec<&Action> {
        match self {
            Action::Chain { steps } => steps.iter().flat_map(Action::iter_leaves).collect(),
            other => vec![other],
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_action_round_trips_through_toml() {
        // Readability is the simplest kind — no schema payload.
        let toml = r#"
type = "Extract"
source = "last_connector_output.body"
[kind]
kind = "readability"
"#;
        let action: Action = toml::from_str(toml).unwrap();
        match action {
            Action::Extract { source, kind } => {
                assert_eq!(source, "last_connector_output.body");
                assert!(matches!(kind, ExtractKind::Readability));
            }
            _ => panic!("expected Extract"),
        }
    }

    #[test]
    fn extract_with_css_schema_round_trips() {
        let toml = r#"
type = "Extract"
source = "last_connector_output.body"
[kind]
kind = "css"
[kind.schema]
title = "h1.article-title"
"#;
        let action: Action = toml::from_str(toml).unwrap();
        match action {
            Action::Extract {
                kind: ExtractKind::Css { schema },
                ..
            } => {
                assert_eq!(schema["title"], "h1.article-title");
            }
            _ => panic!("expected Extract::Css"),
        }
    }

    #[test]
    fn extract_with_jsonpath_schema_round_trips() {
        let toml = r#"
type = "Extract"
source = "last_connector_output.output"
[kind]
kind = "json_path"
[kind.schema]
temp = "$.current_condition[0].temp_C"
desc = "$.current_condition[0].weatherDesc[0].value"
"#;
        let action: Action = toml::from_str(toml).unwrap();
        match action {
            Action::Extract {
                kind: ExtractKind::JsonPath { schema },
                ..
            } => {
                assert_eq!(schema["temp"], "$.current_condition[0].temp_C");
            }
            _ => panic!("expected Extract::JsonPath"),
        }
    }

    #[test]
    fn extract_feed_kind_round_trips() {
        let action = Action::Extract {
            source: "last_connector_output.body".into(),
            kind: ExtractKind::Feed,
        };
        let s = serde_json::to_string(&action).unwrap();
        let back: Action = serde_json::from_str(&s).unwrap();
        match back {
            Action::Extract {
                kind: ExtractKind::Feed,
                ..
            } => {}
            _ => panic!("expected Feed kind round-trip"),
        }
    }

    #[test]
    fn extract_ical_kind_round_trips_with_window() {
        let action = Action::Extract {
            source: "last_connector_output.body".into(),
            kind: ExtractKind::Ical {
                window_days: Some(60),
            },
        };
        let s = serde_json::to_string(&action).unwrap();
        let back: Action = serde_json::from_str(&s).unwrap();
        match back {
            Action::Extract {
                kind: ExtractKind::Ical { window_days },
                ..
            } => {
                assert_eq!(window_days, Some(60));
            }
            _ => panic!("expected Ical kind round-trip"),
        }
    }

    #[test]
    fn extract_llm_schema_round_trips() {
        let action = Action::Extract {
            source: "last_connector_output.body".into(),
            kind: ExtractKind::LlmSchema {
                schema: json!({ "type": "object", "properties": { "price": { "type": "number" } } }),
                hint: Some("Extract product price".into()),
            },
        };
        let s = serde_json::to_string(&action).unwrap();
        let back: Action = serde_json::from_str(&s).unwrap();
        match back {
            Action::Extract {
                kind: ExtractKind::LlmSchema { schema, hint },
                ..
            } => {
                assert_eq!(schema["type"], "object");
                assert_eq!(hint.as_deref(), Some("Extract product price"));
            }
            _ => panic!("expected LlmSchema round-trip"),
        }
    }

    #[test]
    fn dedupe_action_round_trips_through_toml() {
        let toml = r#"
type = "Dedupe"
key = "${last_extract_output.entries.0.id}"
bucket = "rss-broadcast"
history = 5000
"#;
        let action: Action = toml::from_str(toml).unwrap();
        match action {
            Action::Dedupe {
                key,
                bucket,
                history,
            } => {
                assert_eq!(key, "${last_extract_output.entries.0.id}");
                assert_eq!(bucket, "rss-broadcast");
                assert_eq!(history, 5000);
            }
            _ => panic!("expected Dedupe"),
        }
    }

    #[test]
    fn dedupe_history_defaults_to_10k() {
        let toml = r#"
type = "Dedupe"
key = "${trigger.id}"
bucket = "page-watcher"
"#;
        let action: Action = toml::from_str(toml).unwrap();
        match action {
            Action::Dedupe { history, .. } => {
                assert_eq!(history, default_dedupe_history());
                assert_eq!(history, 10_000);
            }
            _ => panic!("expected Dedupe"),
        }
    }

    #[test]
    fn passthrough_extract_round_trips() {
        let action = Action::Extract {
            source: "last_connector_output.output".into(),
            kind: ExtractKind::Passthrough,
        };
        let s = serde_json::to_string(&action).unwrap();
        let back: Action = serde_json::from_str(&s).unwrap();
        assert!(
            matches!(
                back,
                Action::Extract {
                    kind: ExtractKind::Passthrough,
                    ..
                }
            ),
            "passthrough round-trip failed"
        );
    }
}
