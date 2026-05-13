//! Errors returned by [`super::StructuredExtractor`].
//!
//! Distinct variants are an audit-log requirement: a model
//! `Refused` is a terminal decision that must surface separately
//! from `OutputInvalid` ("model tried and got the shape wrong") so
//! the executions log can render a clean reason. OpenAI's documented
//! guidance specifically warns that `null parsed + populated refusal`
//! is a 403-equivalent and MUST NOT be retried as a parse failure.

use thiserror::Error;

use crate::error::AiError;

/// Failure modes for [`super::StructuredExtractor::extract_structured`].
#[derive(Debug, Error)]
pub enum ExtractorError {
    /// JSON Schema malformed. Caught at preflight ideally — when it
    /// reaches this point the recipe author wrote a schema the
    /// adapter rejected at request time.
    #[error("schema invalid: {0}")]
    SchemaInvalid(String),

    /// Adapter produced output that didn't match the schema after
    /// all retries. Carries the attempt count + the last error so
    /// the executions log records both signals.
    #[error("output did not match schema after {attempts} attempt(s): {last_error}")]
    OutputInvalid { attempts: u8, last_error: String },

    /// Model refused to extract. **Terminal — not retried.**
    /// OpenAI's `response.refusal` field; Anthropic's
    /// `stop_reason == "refusal"`. Surfaced distinctly so the
    /// executions log shows "the model said no" rather than
    /// "the model tried and got it wrong".
    #[error("model refused: {reason}")]
    Refused { reason: String },

    /// Underlying adapter error (HTTP failure, auth, timeout).
    /// Wraps [`AiError`] so the dispatcher can attach the same
    /// telemetry it does for any other adapter call.
    #[error("adapter error: {0}")]
    Adapter(#[from] AiError),

    /// This adapter doesn't implement structured extraction.
    /// Defensive variant — callers routing through trait objects
    /// land here when an adapter that returned `None` from
    /// [`crate::AiAdapter::structured_extractor`] is still reached.
    /// In normal flow preflight catches this earlier.
    #[error("adapter does not support structured extraction")]
    Unsupported,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn refusal_is_distinct_from_output_invalid() {
        // Refused must not be matched by OutputInvalid pattern —
        // the executions log relies on this distinction.
        let refused = ExtractorError::Refused {
            reason: "policy".into(),
        };
        let invalid = ExtractorError::OutputInvalid {
            attempts: 2,
            last_error: "missing field".into(),
        };
        assert!(matches!(refused, ExtractorError::Refused { .. }));
        assert!(matches!(invalid, ExtractorError::OutputInvalid { .. }));
        assert!(!matches!(refused, ExtractorError::OutputInvalid { .. }));
    }

    #[test]
    fn ai_error_converts_into_adapter_variant() {
        let ai_err = AiError::Timeout;
        let extractor_err: ExtractorError = ai_err.into();
        assert!(matches!(extractor_err, ExtractorError::Adapter(_)));
    }
}
