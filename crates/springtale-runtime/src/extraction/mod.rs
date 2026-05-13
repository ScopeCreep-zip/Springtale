//! Extraction ladder — the per-kind handlers behind
//! `Action::Extract`.
//!
//! The dispatcher resolves an [`springtale_core::rule::Action::Extract`]
//! step by reading the `source` reference from the live chain
//! context, then calling [`extract`] with the value + the
//! author-declared [`ExtractKind`]. Each kind is a separate module
//! so adding a new kind is one file + one match arm.
//!
//! ## Layers (per v2 plan §"Extraction ladder")
//!
//! 1. **Readability** ([`readability`]) — main-content extraction,
//!    pure heuristic, no AI required. Highest recall on article
//!    pages.
//! 2. **CSS selectors** ([`css`]) — author writes
//!    `{ field: "selector" }` schemas. Stable when the source's
//!    DOM doesn't change.
//! 3. **JSONPath** ([`jsonpath`]) — author writes
//!    `{ field: "$.path" }` over JSON APIs.
//! 4. **Feed** ([`feed`]) — `feed-rs` unified parser for RSS / Atom
//!    / JSON Feed.
//! 5. **iCalendar** ([`ical`]) — VEVENT + RRULE expansion.
//! 6. **LLM schema** ([`llm_schema`]) — defers to the configured
//!    AI adapter. Phase B activates; today returns
//!    [`error::ExtractError::NoAiAdapter`] so recipes surface a
//!    clear "needs an adapter" message.
//! 7. **Passthrough** — no extraction; pipes the source value as
//!    `last_extract_output` for downstream dedupe / messaging.
//!
//! ## Type policy
//!
//! All handlers return `Result<serde_json::Value, ExtractError>`.
//! Schemas (Css / JsonPath / LlmSchema) carry arbitrary JSON the
//! recipe author posts; per-field validation lives in each handler.

pub mod css;
pub mod diff;
pub mod error;
pub mod feed;
pub mod ical;
pub mod jsonpath;
pub mod llm_schema;
pub mod readability;

pub use error::ExtractError;

use serde_json::Value;
use springtale_core::rule::action::ExtractKind;

/// Dispatch one [`ExtractKind`] over the `source` JSON value. Called
/// by the runtime dispatcher when an
/// [`springtale_core::rule::Action::Extract`] step runs.
///
/// `ai` is the resolved AI adapter for the firing context — used
/// only by [`ExtractKind::LlmSchema`]. Pass `None` to disable AI
/// extraction even when an adapter is configured (preview / dry-run
/// safety).
pub async fn extract(
    source: &Value,
    kind: &ExtractKind,
    ai: Option<&dyn springtale_ai::AiAdapter>,
) -> Result<Value, ExtractError> {
    match kind {
        ExtractKind::Readability => readability::extract(source),
        ExtractKind::Css { schema } => css::extract(source, schema),
        ExtractKind::JsonPath { schema } => jsonpath::extract(source, schema),
        ExtractKind::Feed => feed::extract(source),
        ExtractKind::Ical { window_days } => ical::extract(source, *window_days),
        ExtractKind::LlmSchema { schema, hint } => {
            llm_schema::extract(source, schema, hint.as_deref(), ai).await
        }
        ExtractKind::Passthrough => Ok(source.clone()),
        ExtractKind::PageDiff => diff::hash(source),
    }
}

/// Helper for handlers that need the source as a string. Used by
/// Readability / CSS / Feed / iCal — all of which expect a text
/// body. Returns [`ExtractError::SourceNotString`] when the source
/// is a non-string JSON type so the dispatcher surfaces a clean
/// "wrong upstream shape" error instead of a panic.
pub(crate) fn source_as_str(source: &Value) -> Result<&str, ExtractError> {
    match source {
        Value::String(s) => Ok(s.as_str()),
        Value::Null => Err(ExtractError::SourceNotString { got: "null" }),
        Value::Bool(_) => Err(ExtractError::SourceNotString { got: "bool" }),
        Value::Number(_) => Err(ExtractError::SourceNotString { got: "number" }),
        Value::Array(_) => Err(ExtractError::SourceNotString { got: "array" }),
        Value::Object(_) => Err(ExtractError::SourceNotString { got: "object" }),
    }
}
