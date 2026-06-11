//! Ollama structured extraction via `format: <jsonSchema>`.
//!
//! Ollama 0.5+ accepts a `format` field on `/api/chat` whose value
//! is a JSON Schema object. Server-side llama.cpp uses GBNF grammar
//! constraints to guarantee the response is schema-valid by
//! construction. This is **not** the legacy `format: "json"` mode
//! (which only forces JSON-shaped output, not schema-shaped).
//!
//! ## Token usage
//!
//! Ollama runs locally and doesn't report billable tokens. We do
//! surface the `prompt_eval_count` + `eval_count` fields when the
//! server includes them so the executions log records throughput
//! numbers, but the [`ExtractOutcome::usage`] field is `None` when
//! those counts are absent (older Ollama builds).
//!
//! ## Retry policy
//!
//! Grammar-constrained output should never fail validation. If a
//! local user runs an old Ollama that ignores the schema and only
//! does plain JSON mode, the tripwire in
//! [`crate::extractor::validate`] catches the shape mismatch and
//! returns [`ExtractorError::OutputInvalid`] — which the user can
//! resolve by upgrading their Ollama install.

use async_trait::async_trait;

use crate::adapter::TokenUsage;
use crate::extractor::error::ExtractorError;
use crate::extractor::trait_::{
    ExtractOptions, ExtractOutcome, StructuredExtractor, synthesize_full_confidence,
};

use super::adapter::OllamaAdapter;

#[async_trait]
impl StructuredExtractor for OllamaAdapter {
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
            let body = build_extract_body(
                self.ollama_model(),
                &sanitized_source,
                schema,
                sanitized_hint.as_deref(),
                last_error.as_deref(),
                &options,
            );

            let result =
                tokio::time::timeout(options.timeout, self.ollama_client().chat_raw(&body))
                    .await
                    .map_err(|_| ExtractorError::Adapter(crate::error::AiError::Timeout))??;

            match parse_extract_response(&result, schema) {
                ParseOutcome::Invalid(reason) => {
                    last_error = Some(reason);
                    continue;
                }
                ParseOutcome::Ok {
                    value,
                    usage,
                    model,
                } => {
                    let mut confidence = synthesize_full_confidence(&value);
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
}

fn build_extract_body(
    model: &str,
    source: &str,
    schema: &serde_json::Value,
    hint: Option<&str>,
    last_error: Option<&str>,
    options: &ExtractOptions,
) -> serde_json::Value {
    let mut system_lines = vec![
        "Extract a value that matches the provided JSON schema.".to_owned(),
        "Respond with JSON only.".to_owned(),
    ];
    if let Some(h) = hint {
        system_lines.push(format!("Author hint: {h}"));
    }
    let mut messages: Vec<serde_json::Value> = vec![
        serde_json::json!({ "role": "system", "content": system_lines.join(" ") }),
        serde_json::json!({ "role": "user", "content": source }),
    ];
    if let Some(err) = last_error {
        messages.push(serde_json::json!({
            "role": "user",
            "content": format!(
                "Your previous response did not satisfy the schema: {err}. \
                 Re-emit JSON that matches the schema exactly.",
            ),
        }));
    }

    serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": false,
        "format": schema,
        "options": {
            "temperature": options.temperature,
            "num_predict": options.max_tokens,
        }
    })
}

fn parse_extract_response(result: &serde_json::Value, schema: &serde_json::Value) -> ParseOutcome {
    let content = match result
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
    {
        Some(c) if !c.is_empty() => c,
        _ => return ParseOutcome::Invalid("response missing message.content".into()),
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

    let usage = match (
        result.get("prompt_eval_count").and_then(|v| v.as_u64()),
        result.get("eval_count").and_then(|v| v.as_u64()),
    ) {
        (Some(prompt), Some(completion)) => Some(TokenUsage {
            prompt_tokens: prompt as u32,
            completion_tokens: completion as u32,
            total_tokens: (prompt + completion) as u32,
        }),
        _ => None,
    };

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
    fn body_uses_format_field_not_format_string() {
        let schema = json!({ "type": "object" });
        let body = build_extract_body(
            "llama3.2:3b",
            "source",
            &schema,
            None,
            None,
            &ExtractOptions::default(),
        );
        // Critical: format is the schema object, NOT the string "json".
        // The string form falls back to text-mode JSON, not GBNF-constrained.
        assert!(body["format"].is_object());
        assert_eq!(body["format"]["type"], "object");
        assert_eq!(body["stream"], false);
    }

    #[test]
    fn body_passes_temperature_and_num_predict() {
        let schema = json!({ "type": "object" });
        let opts = ExtractOptions::default();
        let body = build_extract_body("llama3.2:3b", "source", &schema, None, None, &opts);
        assert_eq!(body["options"]["temperature"], 0.0);
        assert_eq!(body["options"]["num_predict"], 4096);
    }

    #[test]
    fn valid_response_parses_with_local_usage() {
        let content = json!({ "title": "x" }).to_string();
        let resp = json!({
            "message": { "content": content },
            "model": "llama3.2:3b",
            "prompt_eval_count": 10,
            "eval_count": 4,
        });
        let schema = json!({
            "type": "object",
            "properties": { "title": { "type": "string" } },
            "required": ["title"]
        });
        match parse_extract_response(&resp, &schema) {
            ParseOutcome::Ok {
                value,
                usage,
                model,
            } => {
                assert_eq!(value["title"], "x");
                assert_eq!(model, "llama3.2:3b");
                let usage = usage.unwrap();
                assert_eq!(usage.prompt_tokens, 10);
                assert_eq!(usage.completion_tokens, 4);
                assert_eq!(usage.total_tokens, 14);
            }
            _ => panic!("expected Ok"),
        }
    }

    #[test]
    fn missing_eval_counts_returns_none_usage() {
        let content = json!({}).to_string();
        let resp = json!({ "message": { "content": content }, "model": "test" });
        let schema = json!({ "type": "object" });
        match parse_extract_response(&resp, &schema) {
            ParseOutcome::Ok { usage, .. } => assert!(usage.is_none()),
            _ => panic!("expected Ok"),
        }
    }

    #[test]
    fn shape_mismatch_marks_invalid() {
        let content = json!({ "other": "x" }).to_string();
        let resp = json!({ "message": { "content": content } });
        let schema = json!({
            "type": "object",
            "required": ["title"]
        });
        assert!(matches!(
            parse_extract_response(&resp, &schema),
            ParseOutcome::Invalid(_)
        ));
    }

    #[test]
    fn retry_message_appended_when_last_error_present() {
        let schema = json!({ "type": "object" });
        let body = build_extract_body(
            "llama3.2:3b",
            "source",
            &schema,
            None,
            Some("missing field x"),
            &ExtractOptions::default(),
        );
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 3);
        assert!(
            messages[2]["content"]
                .as_str()
                .unwrap()
                .contains("missing field x")
        );
    }
}
