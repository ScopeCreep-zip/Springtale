//! Errors surfaced by the extraction ladder. Each kind reports its
//! own failure mode so the dispatcher can attach a meaningful step
//! error to the [`springtale_core::rule::StepOutput`] / executions
//! row.

use thiserror::Error;

/// Failure modes for [`super::extract`].
#[derive(Debug, Error)]
pub enum ExtractError {
    /// Source value not a string (Readability / CSS / Feed / iCal
    /// all expect a text body).
    #[error("extract source is not a string (got {got})")]
    SourceNotString { got: &'static str },

    /// Readability failed to parse the HTML or returned no article.
    #[error("readability parse failed: {0}")]
    Readability(String),

    /// scraper / html5ever rejected one of the author's CSS selectors.
    #[error("invalid CSS selector `{selector}`: {reason}")]
    CssSelector { selector: String, reason: String },

    /// `serde_json_path` rejected one of the author's JSONPath
    /// expressions, or the source wasn't valid JSON.
    #[error("invalid JSONPath expression `{path}`: {reason}")]
    JsonPath { path: String, reason: String },

    /// `feed-rs` couldn't parse the source as RSS/Atom/JSON Feed.
    #[error("feed parse failed: {0}")]
    Feed(String),

    /// `icalendar` couldn't parse the source as a VCALENDAR.
    #[error("ical parse failed: {0}")]
    Ical(String),

    /// LLM-driven extraction requested but no AI adapter is
    /// configured (NoopAdapter is the safe default). The runtime
    /// surfaces this so the recipe author knows which tier of the
    /// ladder requires an adapter.
    #[error("LLM extraction requested but no AI adapter is configured")]
    NoAiAdapter,

    /// LLM call ran but returned content that didn't satisfy the
    /// declared schema.
    #[error("LLM extraction failed: {0}")]
    Llm(String),

    /// Author-declared schema field has an unsupported type.
    #[error("schema field `{field}` has unsupported value type")]
    SchemaFieldType { field: String },
}
