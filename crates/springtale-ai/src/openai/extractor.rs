//! OpenAI strict `json_schema` structured extraction.
//!
//! Uses `response_format: { type: "json_schema", json_schema:
//! { name, schema, strict: true } }` on `/v1/chat/completions`.
//! Strict mode requires `gpt-4o-2024-08-06` or newer; older models
//! silently fall back to plain JSON mode which is NOT
//! grammar-constrained. The adapter doesn't gate on model name
//! because OpenAI-compatible endpoints (DeepSeek, OpenRouter, vLLM,
//! llama.cpp server) implement the same API surface — the user's
//! config is the source of truth for capability.
//!
//! ## Refusal handling
//!
//! OpenAI 2024-08-06+ adds a `message.refusal` field. When set, it
//! is the model's safety-system response and `message.content` is
//! `null`. We surface this as [`ExtractorError::Refused`] —
//! **terminal, never retried** — so the executions log can render
//! a clean reason. This matches OpenAI's own structured-outputs
//! guidance ("Always check for refusal before parsing parsed").
//!
//! ## Retry policy
//!
//! Strict mode is grammar-valid by construction, so retries should
//! never fire on a healthy endpoint. If `OutputInvalid` does occur
//! (older compat endpoint claiming strict support without the
//! grammar constraint), [`ExtractOptions::max_retries`] applies.
//! Default `1` retry. Constrained decoding adapters expect 0.

use async_trait::async_trait;

use crate::adapter::TokenUsage;
use crate::extractor::error::ExtractorError;
use crate::extractor::trait_::{
    synthesize_full_confidence, ExtractOptions, ExtractOutcome, StructuredExtractor,
};

use super::adapter::OpenAiCompatAdapter;

#[async_trait]
impl StructuredExtractor for OpenAiCompatAdapter {
    async fn extract_structured(
        &self,
        source: &str,
        schema: &serde_json::Value,
        hint: Option<&str>,
        options: ExtractOptions,
    ) -> Result<ExtractOutcome, ExtractorError> {
        if !schema.is_object() {
            return Err(ExtractorError::SchemaInvalid(
                "expected JSON Schema object".into(),
            ));
        }
        // Sanitize the source through the adapter's sanitizer
        // boundary (Layer 2 defense). The same boundary completion
        // calls cross.
        let sanitized_source = self
            .sanitize_for_extractor("extract.source", source)
            .map_err(ExtractorError::Adapter)?;
        let sanitized_hint = match hint {
            Some(h) => Some(
                self.sanitize_for_extractor("extract.hint", h)
                    .map_err(ExtractorError::Adapter)?,
            ),
            None => None,
        };

        let mut last_error: Option<String> = None;
        let attempts = options.max_retries.saturating_add(1);

        for attempt in 0..attempts {
            let body = self.build_extract_body(
                &sanitized_source,
                schema,
                sanitized_hint.as_deref(),
                last_error.as_deref(),
                &options,
            );

            let result = tokio::time::timeout(
                options.timeout,
                self.openai_client().chat_completion(&body),
            )
            .await
            .map_err(|_| ExtractorError::Adapter(crate::error::AiError::Timeout))??;

            match parse_extract_response(&result, schema) {
                ParseOutcome::Refused(reason) => {
                    // Terminal — refusals are never retried.
                    return Err(ExtractorError::Refused { reason });
                }
                ParseOutcome::Invalid(reason) => {
                    last_error = Some(reason);
                    continue;
                }
                ParseOutcome::Ok { value, usage, model } => {
                    let mut confidence = synthesize_full_confidence(&value);
                    // First-try success: leave 1.0 across the board.
                    // Retry success: drop the score so the executions
                    // log records the retry as a drift signal.
                    if attempt > 0 {
                        for c in confidence.values_mut() {
                            *c = 0.7;
                        }
                    }
                    return Ok(ExtractOutcome {
                        value,
                        field_confidence: confidence,
                        model,
                        usage,
                        retries: attempt,
                    });
                }
            }
        }

        Err(ExtractorError::OutputInvalid {
            attempts,
            last_error: last_error
                .unwrap_or_else(|| "schema validation failed (no error captured)".into()),
        })
    }
}

enum ParseOutcome {
    Ok {
        value: serde_json::Value,
        usage: Option<TokenUsage>,
        model: String,
    },
    Invalid(String),
    Refused(String),
}

fn parse_extract_response(result: &serde_json::Value, schema: &serde_json::Value) -> ParseOutcome {
    let choice = result.get("choices").and_then(|c| c.get(0));
    let message = match choice.and_then(|c| c.get("message")) {
        Some(m) => m,
        None => return ParseOutcome::Invalid("response missing choices[0].message".into()),
    };

    if let Some(refusal) = message.get("refusal").and_then(|r| r.as_str()) {
        if !refusal.is_empty() {
            // Truncate to 256 chars — the audit-trail invariant
            // caps the executions-log refusal payload at the same
            // limit so the privacy boundary holds.
            let truncated: String = refusal.chars().take(256).collect();
            return ParseOutcome::Refused(truncated);
        }
    }

    let content = match message.get("content").and_then(|c| c.as_str()) {
        Some(c) if !c.is_empty() => c,
        _ => return ParseOutcome::Invalid("response message.content missing".into()),
    };

    let value: serde_json::Value = match serde_json::from_str(content) {
        Ok(v) => v,
        Err(e) => return ParseOutcome::Invalid(format!("response content not valid JSON: {e}")),
    };

    if let Err(reason) = crate::extractor::validate::validate_against(&value, schema) {
        return ParseOutcome::Invalid(reason);
    }

    let model = result
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("")
        .to_owned();
    let usage = result.get("usage").and_then(|u| {
        Some(TokenUsage {
            prompt_tokens: u.get("prompt_tokens")?.as_u64()? as u32,
            completion_tokens: u.get("completion_tokens")?.as_u64()? as u32,
            total_tokens: u.get("total_tokens")?.as_u64()? as u32,
        })
    });

    ParseOutcome::Ok {
        value,
        usage,
        model,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn refusal_field_routes_to_refused() {
        let resp = json!({
            "choices": [{
                "message": { "refusal": "I cannot extract personal data", "content": null }
            }],
            "model": "gpt-4o-2024-08-06"
        });
        let schema = json!({ "type": "object" });
        match parse_extract_response(&resp, &schema) {
            ParseOutcome::Refused(reason) => {
                assert!(reason.contains("personal data"));
            }
            _ => panic!("expected Refused"),
        }
    }

    #[test]
    fn refusal_truncated_to_256_chars() {
        let long = "x".repeat(500);
        let resp = json!({
            "choices": [{ "message": { "refusal": long } }],
        });
        let schema = json!({ "type": "object" });
        match parse_extract_response(&resp, &schema) {
            ParseOutcome::Refused(reason) => {
                assert_eq!(reason.chars().count(), 256);
            }
            _ => panic!("expected Refused"),
        }
    }

    #[test]
    fn well_formed_response_parses() {
        let content = json!({ "title": "x" }).to_string();
        let resp = json!({
            "choices": [{ "message": { "content": content } }],
            "model": "gpt-4o-2024-08-06",
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 4,
                "total_tokens": 14
            }
        });
        let schema = json!({
            "type": "object",
            "properties": { "title": { "type": "string" } },
            "required": ["title"]
        });
        match parse_extract_response(&resp, &schema) {
            ParseOutcome::Ok { value, usage, model } => {
                assert_eq!(value["title"], "x");
                assert_eq!(model, "gpt-4o-2024-08-06");
                let usage = usage.unwrap();
                assert_eq!(usage.prompt_tokens, 10);
                assert_eq!(usage.completion_tokens, 4);
            }
            _ => panic!("expected Ok"),
        }
    }

    #[test]
    fn missing_required_field_marks_invalid() {
        let content = json!({ "other": "y" }).to_string();
        let resp = json!({
            "choices": [{ "message": { "content": content } }]
        });
        let schema = json!({
            "type": "object",
            "properties": { "title": { "type": "string" } },
            "required": ["title"]
        });
        assert!(matches!(
            parse_extract_response(&resp, &schema),
            ParseOutcome::Invalid(_)
        ));
    }

    #[test]
    fn invalid_json_content_marks_invalid() {
        let resp = json!({
            "choices": [{ "message": { "content": "{not json" } }]
        });
        let schema = json!({ "type": "object" });
        assert!(matches!(
            parse_extract_response(&resp, &schema),
            ParseOutcome::Invalid(_)
        ));
    }
}
