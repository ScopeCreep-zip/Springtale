//! LLM-driven structured extraction.
//!
//! Routes to the configured AI adapter's
//! [`springtale_ai::StructuredExtractor`] capability. Phase B
//! activates this — the dispatcher resolves the adapter via
//! [`crate::cooperation::CapabilityBridge::ai_adapter_for`] and
//! passes it down, exactly the same socket the recipe author
//! configured.
//!
//! ## AI-optional invariant
//!
//! Three doors out without an adapter:
//!
//! 1. `ai = None` (no adapter configured) →
//!    [`ExtractError::NoAiAdapter`].
//! 2. Adapter present but `structured_extractor()` returns `None`
//!    (e.g. [`springtale_ai::NoopAdapter`], or an adapter that
//!    doesn't support constrained decoding) →
//!    [`ExtractError::NoAiAdapter`].
//! 3. Adapter call returns
//!    [`springtale_ai::ExtractorError::Unsupported`] →
//!    [`ExtractError::NoAiAdapter`].
//!
//! All three resolve to the same surface: the recipe author sees a
//! clean "this tier needs an AI adapter" message, NOT a silent
//! null. Preflight (Phase B.4 cap check) catches doors 1–2 at
//! deploy time so the user never reaches runtime in that state.

use serde_json::Value;
use springtale_ai::{AiAdapter, ExtractOptions, ExtractorError};

use super::error::ExtractError;
use super::source_as_str;

pub async fn extract(
    source: &Value,
    schema: &Value,
    hint: Option<&str>,
    ai: Option<&dyn AiAdapter>,
) -> Result<Value, ExtractError> {
    let adapter = ai.ok_or(ExtractError::NoAiAdapter)?;
    let extractor = adapter
        .structured_extractor()
        .ok_or(ExtractError::NoAiAdapter)?;

    let text = source_as_str(source)?;

    let outcome = extractor
        .extract_structured(text, schema, hint, ExtractOptions::default())
        .await
        .map_err(|e| match e {
            ExtractorError::Unsupported => ExtractError::NoAiAdapter,
            ExtractorError::Refused { reason } => {
                ExtractError::Llm(format!("model refused: {reason}"))
            }
            ExtractorError::SchemaInvalid(reason) => {
                ExtractError::Llm(format!("schema invalid: {reason}"))
            }
            ExtractorError::OutputInvalid {
                attempts,
                last_error,
            } => ExtractError::Llm(format!(
                "extraction failed after {attempts} attempt(s): {last_error}"
            )),
            ExtractorError::Adapter(ai_err) => ExtractError::Llm(ai_err.to_string()),
        })?;

    // The chain context records `outcome.value`; the audit-trail
    // fields (model / usage / retries / field_confidence) flow into
    // the executions log via the recorder when B.6 lands.
    Ok(outcome.value)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use serde_json::json;
    use springtale_ai::adapter::{
        AiOptions, AiRequest, AiResponse, AiStream, ConnectorInfo,
    };
    use springtale_ai::{
        AiError, ExtractOptions, ExtractOutcome, NoopAdapter, StructuredExtractor,
    };
    use std::collections::BTreeMap;
    use springtale_core::rule::types::Rule;

    /// Test adapter that returns a fixed structured response.
    struct FakeExtractAdapter {
        value: serde_json::Value,
    }

    #[async_trait]
    impl AiAdapter for FakeExtractAdapter {
        async fn complete(
            &self,
            _request: AiRequest,
            _options: AiOptions,
        ) -> Result<AiResponse, AiError> {
            Err(AiError::Disabled)
        }
        async fn stream(
            &self,
            _request: AiRequest,
            _options: AiOptions,
        ) -> Result<AiStream, AiError> {
            Err(AiError::Disabled)
        }
        async fn parse_rule(
            &self,
            _intent: &str,
            _connectors: &[ConnectorInfo],
        ) -> Result<Rule, AiError> {
            Err(AiError::Disabled)
        }
        async fn is_available(&self) -> bool {
            true
        }
        fn structured_extractor(&self) -> Option<&dyn StructuredExtractor> {
            Some(self)
        }
    }

    #[async_trait]
    impl StructuredExtractor for FakeExtractAdapter {
        async fn extract_structured(
            &self,
            _source: &str,
            _schema: &serde_json::Value,
            _hint: Option<&str>,
            _options: ExtractOptions,
        ) -> Result<ExtractOutcome, ExtractorError> {
            Ok(ExtractOutcome {
                value: self.value.clone(),
                field_confidence: BTreeMap::new(),
                model: "fake-model".into(),
                usage: None,
                retries: 0,
            })
        }
    }

    /// Test adapter that always refuses.
    struct RefusingAdapter;

    #[async_trait]
    impl AiAdapter for RefusingAdapter {
        async fn complete(
            &self,
            _request: AiRequest,
            _options: AiOptions,
        ) -> Result<AiResponse, AiError> {
            Err(AiError::Disabled)
        }
        async fn stream(
            &self,
            _request: AiRequest,
            _options: AiOptions,
        ) -> Result<AiStream, AiError> {
            Err(AiError::Disabled)
        }
        async fn parse_rule(
            &self,
            _intent: &str,
            _connectors: &[ConnectorInfo],
        ) -> Result<Rule, AiError> {
            Err(AiError::Disabled)
        }
        async fn is_available(&self) -> bool {
            true
        }
        fn structured_extractor(&self) -> Option<&dyn StructuredExtractor> {
            Some(self)
        }
    }

    #[async_trait]
    impl StructuredExtractor for RefusingAdapter {
        async fn extract_structured(
            &self,
            _source: &str,
            _schema: &serde_json::Value,
            _hint: Option<&str>,
            _options: ExtractOptions,
        ) -> Result<ExtractOutcome, ExtractorError> {
            Err(ExtractorError::Refused {
                reason: "policy".into(),
            })
        }
    }

    #[tokio::test]
    async fn no_adapter_returns_no_ai_adapter_error() {
        let err = extract(&json!("html"), &json!({}), None, None)
            .await
            .unwrap_err();
        assert!(matches!(err, ExtractError::NoAiAdapter));
    }

    #[tokio::test]
    async fn noop_adapter_returns_no_ai_adapter_error() {
        // NoopAdapter's structured_extractor() returns None — the
        // AI-optional invariant in action. Recipes that try
        // ExtractKind::LlmSchema with NoopAdapter configured land
        // here.
        let adapter = NoopAdapter;
        let err = extract(&json!("html"), &json!({}), None, Some(&adapter))
            .await
            .unwrap_err();
        assert!(matches!(err, ExtractError::NoAiAdapter));
    }

    #[tokio::test]
    async fn working_adapter_returns_value() {
        let adapter = FakeExtractAdapter {
            value: json!({ "title": "x" }),
        };
        let value = extract(
            &json!("body"),
            &json!({ "type": "object" }),
            None,
            Some(&adapter),
        )
        .await
        .unwrap();
        assert_eq!(value["title"], "x");
    }

    #[tokio::test]
    async fn refusal_surfaces_as_llm_error() {
        let adapter = RefusingAdapter;
        let err = extract(&json!("body"), &json!({}), None, Some(&adapter))
            .await
            .unwrap_err();
        match err {
            ExtractError::Llm(msg) => assert!(msg.contains("refused")),
            _ => panic!("expected Llm refused error"),
        }
    }
}
