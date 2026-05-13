//! Anthropic structured extraction via **forced tool use**.
//!
//! The portable path that works across the full Claude family
//! (Sonnet 3.5+, 4.x, Opus 4.x). The newer
//! `anthropic-beta: structured-outputs-2025-11-13` header
//! activates native constrained decoding on Sonnet 4.6 / Opus 4.7
//! but requires a per-deployment opt-in — keeping the forced-tool
//! path as the default makes the extractor work out-of-the-box
//! against any Anthropic-compatible endpoint the user configures.
//!
//! ## Request shape
//!
//! - `tools: [{ name: "extract", input_schema: <schema> }]`
//! - `tool_choice: { type: "tool", name: "extract" }`
//!
//! Anthropic guarantees the model emits exactly one `tool_use`
//! content block whose `input` matches `input_schema`. Extra
//! `text` blocks may appear alongside (commentary, refusals,
//! preamble) — we surface refusals separately and pull the
//! `tool_use` input as the extracted value.
//!
//! ## Refusal handling
//!
//! `stop_reason == "refusal"` is terminal. Older models pre-refusal
//! turn-completion semantics can also emit text-only responses
//! when the model declines without setting the stop_reason — we
//! treat "no tool_use block found" with a `text` payload as a
//! soft refusal and surface the truncated text in
//! [`ExtractorError::Refused`].
//!
//! ## Retry policy
//!
//! Forced tool use is grammar-valid in principle but pre-4.6
//! Sonnet models occasionally drop required fields. One retry by
//! default (the [`ExtractOptions::max_retries`] default). The
//! retry feedback echo the validator's error so the model gets
//! grounded guidance rather than a bare "try again".

use async_trait::async_trait;

use crate::adapter::TokenUsage;
use crate::extractor::error::ExtractorError;
use crate::extractor::trait_::{
    synthesize_full_confidence, ExtractOptions, ExtractOutcome, StructuredExtractor,
};

use super::adapter::AnthropicAdapter;

#[async_trait]
impl StructuredExtractor for AnthropicAdapter {
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
                self.anthropic_model(),
                &sanitized_source,
                schema,
                sanitized_hint.as_deref(),
                last_error.as_deref(),
                &options,
            );

            let result = tokio::time::timeout(
                options.timeout,
                self.anthropic_client().messages(&body),
            )
            .await
            .map_err(|_| ExtractorError::Adapter(crate::error::AiError::Timeout))??;

            match parse_extract_response(&result, schema) {
                ParseOutcome::Refused(reason) => {
                    return Err(ExtractorError::Refused { reason });
                }
                ParseOutcome::Invalid(reason) => {
                    last_error = Some(reason);
                    continue;
                }
                ParseOutcome::Ok { value, usage, model } => {
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
    Refused(String),
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
        "Use the `extract` tool to emit a value matching its input_schema.".to_owned(),
        "Do not include explanatory text outside the tool call.".to_owned(),
    ];
    if let Some(h) = hint {
        system_lines.push(format!("Author hint: {h}"));
    }
    let system = system_lines.join(" ");

    let mut messages: Vec<serde_json::Value> = vec![serde_json::json!({
        "role": "user",
        "content": source,
    })];
    if let Some(err) = last_error {
        messages.push(serde_json::json!({
            "role": "user",
            "content": format!(
                "The previous tool input did not satisfy the schema: {err}. \
                 Re-invoke `extract` with input that matches exactly.",
            ),
        }));
    }

    serde_json::json!({
        "model": model,
        "system": system,
        "messages": messages,
        "max_tokens": options.max_tokens,
        "temperature": options.temperature,
        "tools": [{
            "name": "extract",
            "description": "Emit the extracted value matching the declared schema.",
            "input_schema": schema,
        }],
        "tool_choice": { "type": "tool", "name": "extract" },
    })
}

fn parse_extract_response(result: &serde_json::Value, schema: &serde_json::Value) -> ParseOutcome {
    let stop_reason = result
        .get("stop_reason")
        .and_then(|s| s.as_str())
        .unwrap_or("");
    if stop_reason == "refusal" {
        // Capture any text block as the refusal reason. If there's
        // none, fall back to the stop_reason itself.
        let reason = first_text_block(result)
            .map(|t| truncate_256(&t))
            .unwrap_or_else(|| "model refused (no detail)".to_owned());
        return ParseOutcome::Refused(reason);
    }

    let blocks = match result.get("content").and_then(|c| c.as_array()) {
        Some(b) => b,
        None => return ParseOutcome::Invalid("response missing content[]".into()),
    };

    let mut tool_input: Option<&serde_json::Value> = None;
    let mut text_parts: Vec<&str> = Vec::new();
    for block in blocks {
        match block.get("type").and_then(|t| t.as_str()).unwrap_or("") {
            "tool_use" => {
                let name = block.get("name").and_then(|n| n.as_str()).unwrap_or("");
                if name == "extract" {
                    tool_input = block.get("input");
                }
            }
            "text" => {
                if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                    text_parts.push(t);
                }
            }
            _ => {}
        }
    }

    let value = match tool_input {
        Some(v) => v.clone(),
        None => {
            // No tool_use block — treat the text payload as a soft
            // refusal, otherwise generic invalid.
            if !text_parts.is_empty() {
                let combined = text_parts.join("\n");
                return ParseOutcome::Refused(truncate_256(&combined));
            }
            return ParseOutcome::Invalid("response had no `extract` tool_use block".into());
        }
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
        let prompt = u.get("input_tokens")?.as_u64()? as u32;
        let completion = u.get("output_tokens")?.as_u64()? as u32;
        Some(TokenUsage {
            prompt_tokens: prompt,
            completion_tokens: completion,
            total_tokens: prompt + completion,
        })
    });

    ParseOutcome::Ok {
        value,
        usage,
        model,
    }
}

fn first_text_block(result: &serde_json::Value) -> Option<String> {
    let blocks = result.get("content").and_then(|c| c.as_array())?;
    for block in blocks {
        if block.get("type").and_then(|t| t.as_str()) == Some("text") {
            if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                return Some(t.to_owned());
            }
        }
    }
    None
}

fn truncate_256(s: &str) -> String {
    s.chars().take(256).collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tool_use_block_parses_input() {
        let resp = json!({
            "content": [
                { "type": "tool_use", "name": "extract", "input": { "title": "hello" } }
            ],
            "model": "claude-sonnet-4-6",
            "stop_reason": "tool_use",
            "usage": { "input_tokens": 12, "output_tokens": 3 }
        });
        let schema = json!({
            "type": "object",
            "properties": { "title": { "type": "string" } },
            "required": ["title"]
        });
        match parse_extract_response(&resp, &schema) {
            ParseOutcome::Ok { value, usage, model } => {
                assert_eq!(value["title"], "hello");
                assert_eq!(model, "claude-sonnet-4-6");
                let usage = usage.unwrap();
                assert_eq!(usage.prompt_tokens, 12);
                assert_eq!(usage.total_tokens, 15);
            }
            _ => panic!("expected Ok"),
        }
    }

    #[test]
    fn refusal_stop_reason_routes_to_refused() {
        let resp = json!({
            "stop_reason": "refusal",
            "content": [
                { "type": "text", "text": "I can't extract that." }
            ]
        });
        let schema = json!({ "type": "object" });
        match parse_extract_response(&resp, &schema) {
            ParseOutcome::Refused(reason) => assert!(reason.contains("can't extract")),
            _ => panic!("expected Refused"),
        }
    }

    #[test]
    fn text_only_response_treated_as_soft_refusal() {
        let resp = json!({
            "stop_reason": "end_turn",
            "content": [{ "type": "text", "text": "I shouldn't do that." }]
        });
        let schema = json!({ "type": "object" });
        assert!(matches!(
            parse_extract_response(&resp, &schema),
            ParseOutcome::Refused(_)
        ));
    }

    #[test]
    fn missing_required_field_marks_invalid() {
        let resp = json!({
            "content": [
                { "type": "tool_use", "name": "extract", "input": { "other": "x" } }
            ],
            "stop_reason": "tool_use"
        });
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
    fn body_pins_tool_choice() {
        let schema = json!({ "type": "object" });
        let body = build_extract_body(
            "claude-sonnet-4-6",
            "source",
            &schema,
            Some("hint"),
            None,
            &ExtractOptions::default(),
        );
        assert_eq!(body["tool_choice"]["type"], "tool");
        assert_eq!(body["tool_choice"]["name"], "extract");
        assert_eq!(body["tools"][0]["name"], "extract");
        assert!(body["system"].as_str().unwrap().contains("hint"));
    }

    #[test]
    fn body_includes_last_error_on_retry() {
        let schema = json!({ "type": "object" });
        let body = build_extract_body(
            "claude-sonnet-4-6",
            "source",
            &schema,
            None,
            Some("missing field x"),
            &ExtractOptions::default(),
        );
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 2);
        assert!(messages[1]["content"]
            .as_str()
            .unwrap()
            .contains("missing field x"));
    }
}
