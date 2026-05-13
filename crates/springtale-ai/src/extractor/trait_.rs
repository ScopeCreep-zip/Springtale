//! `StructuredExtractor` trait — schema-constrained JSON extraction.
//!
//! Adapters that support constrained decoding (OpenAI strict
//! `json_schema`, Anthropic forced tool use / structured outputs,
//! Ollama `format: <schema>`) implement this trait. Capability
//! discovery happens on [`crate::AiAdapter::structured_extractor`] —
//! the default returns `None`, so [`crate::NoopAdapter`] and any
//! adapter that hasn't grown structured-output support skip it
//! cleanly.
//!
//! This is the "AI-optional" invariant in trait form. Recipes that
//! request [`springtale_core::rule::action::ExtractKind::LlmSchema`]
//! fail preflight (not runtime) when the bound adapter returns
//! `None` for `structured_extractor()`. Recipes that use any other
//! extractor tier (Readability / CSS / JSONPath / Feed / iCal /
//! PageDiff / Passthrough) work without an adapter.

use std::collections::BTreeMap;
use std::time::Duration;

use async_trait::async_trait;

use super::error::ExtractorError;
use crate::adapter::TokenUsage;

/// Schema-constrained JSON extraction.
///
/// Each implementing adapter translates the call into its vendor's
/// constrained-decoding feature:
///
/// | Vendor    | Mechanism                                              |
/// |-----------|--------------------------------------------------------|
/// | OpenAI    | `response_format: { type: "json_schema", strict: true }`|
/// | Anthropic | Forced tool use (or `structured-outputs-2025-11-13` beta) |
/// | Ollama    | `format: <jsonSchema>` on `/api/chat` (NOT `format: "json"`) |
/// | Noop      | Does not implement — `structured_extractor()` returns `None` |
///
/// **Audit trail invariant.** Callers record sizes / hashes / token
/// counts from [`ExtractOutcome`] — never the prompt, never the
/// source text, never the extracted content. Privacy default of the
/// executions log is the stricter baseline.
#[async_trait]
pub trait StructuredExtractor: Send + Sync {
    /// Extract a value matching `schema` from `source`.
    ///
    /// `hint` is an optional natural-language instruction the
    /// adapter forwards as a system / instruction message. It is
    /// always *additive* — the schema is the source of truth.
    async fn extract_structured(
        &self,
        source: &str,
        schema: &serde_json::Value,
        hint: Option<&str>,
        options: ExtractOptions,
    ) -> Result<ExtractOutcome, ExtractorError>;
}

/// Options controlling a structured extraction call.
#[derive(Debug, Clone)]
pub struct ExtractOptions {
    /// Maximum retries on [`ExtractorError::OutputInvalid`]. Default 1.
    ///
    /// Constrained-decoding adapters (OpenAI strict json_schema,
    /// Ollama `format: <schema>`, Anthropic structured-outputs beta)
    /// bypass retry — output is grammar-valid by construction.
    /// Retry exists for the Anthropic forced-tool-use fallback on
    /// older Claude models where the model can still emit
    /// extra `text` blocks alongside the `tool_use` block.
    pub max_retries: u8,
    /// Wall-clock timeout for the underlying adapter call.
    /// Default 30 seconds — matches [`crate::AiOptions::default`].
    pub timeout: Duration,
    /// Sampling temperature passed through to the adapter.
    /// Extraction wants deterministic output, so we default `0.0`
    /// rather than letting the adapter's default sampling fire.
    pub temperature: f32,
    /// Maximum tokens to generate. Default 4096 — enough for the
    /// kinds of structured documents recipes extract.
    pub max_tokens: u32,
}

impl Default for ExtractOptions {
    fn default() -> Self {
        Self {
            max_retries: 1,
            timeout: Duration::from_secs(30),
            temperature: 0.0,
            max_tokens: 4096,
        }
    }
}

/// Result of a structured extraction call.
///
/// The shape mirrors what enterprise extraction systems (UiPath
/// Forms AI, Hyperscience, AWS Textract) return: a value plus
/// per-field confidence, plus enough adapter audit fields to drive
/// drift detection later. Callers don't need the full struct —
/// most consume just `.value`; the executions log records the
/// rest as sizes / hashes / counters.
#[derive(Debug, Clone)]
pub struct ExtractOutcome {
    /// The extracted JSON. Already validated against the schema by
    /// the adapter (constrained decoding) or by the trait
    /// implementation (forced-tool-use fallback) before returning.
    pub value: serde_json::Value,
    /// Per-field confidence in `[0.0, 1.0]`. Synthesized from
    /// schema satisfaction and (when the adapter exposes them)
    /// token logprobs. Recipes don't consume this directly today —
    /// the executions log records `min` / `mean` for drift signal.
    pub field_confidence: BTreeMap<String, f32>,
    /// Provider-side model identifier — `"gpt-4o-2024-08-06"`,
    /// `"claude-sonnet-4-6"`, `"llama3.2:3b"`. Audit only.
    pub model: String,
    /// Token usage when the adapter reports it. `None` for Ollama
    /// (local; not billed / surfaced).
    pub usage: Option<TokenUsage>,
    /// Attempts needed before success. `0` = first-try.
    /// Constrained-decoding adapters always `0`. Used as a drift
    /// signal — sudden spike means schema needs revision.
    pub retries: u8,
}

impl ExtractOutcome {
    /// First-try success at full confidence — what constrained
    /// decoding adapters return on the happy path.
    pub fn first_try(
        value: serde_json::Value,
        model: impl Into<String>,
        usage: Option<TokenUsage>,
    ) -> Self {
        let confidence = synthesize_full_confidence(&value);
        Self {
            value,
            field_confidence: confidence,
            model: model.into(),
            usage,
            retries: 0,
        }
    }
}

/// Walk the top-level fields of `value` and synthesize a
/// per-field confidence map. Constrained-decoding adapters land
/// here — every field gets `1.0` since the grammar guarantees
/// satisfaction. Used by [`ExtractOutcome::first_try`].
pub(crate) fn synthesize_full_confidence(value: &serde_json::Value) -> BTreeMap<String, f32> {
    let mut out = BTreeMap::new();
    if let Some(obj) = value.as_object() {
        for key in obj.keys() {
            out.insert(key.clone(), 1.0);
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_options_defaults_are_deterministic() {
        let opts = ExtractOptions::default();
        assert_eq!(opts.temperature, 0.0);
        assert_eq!(opts.max_retries, 1);
        assert_eq!(opts.max_tokens, 4096);
        assert_eq!(opts.timeout, Duration::from_secs(30));
    }

    #[test]
    fn first_try_outcome_has_full_confidence_per_field() {
        let value = json!({ "title": "x", "score": 7 });
        let outcome = ExtractOutcome::first_try(value.clone(), "gpt-4o-2024-08-06", None);
        assert_eq!(outcome.retries, 0);
        assert_eq!(outcome.field_confidence.get("title").copied(), Some(1.0));
        assert_eq!(outcome.field_confidence.get("score").copied(), Some(1.0));
        assert_eq!(outcome.value, value);
    }

    #[test]
    fn first_try_outcome_with_non_object_value_has_empty_confidence_map() {
        let value = json!(["one", "two"]);
        let outcome = ExtractOutcome::first_try(value, "test-model", None);
        assert!(outcome.field_confidence.is_empty());
    }
}
