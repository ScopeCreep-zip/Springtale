use crate::adapter::{AiAdapter, AiOptions, AiRequest, ChatMessage, ConnectorInfo};
use crate::error::AiError;
use springtale_core::rule::types::{Rule, RuleId, RuleStatus};

use super::prompt;

/// Natural language to Rule parser.
///
/// Builds a prompt from the user's intent + available connector schemas,
/// sends it to the configured AI adapter, and parses the response as
/// a TOML Rule. The rule is always created in `Draft` status — the
/// user must review and enable it manually.
pub struct NlRuleParser;

impl NlRuleParser {
    /// Parse a natural language intent into a structured Rule.
    ///
    /// The AI adapter does the heavy lifting: it receives a system prompt
    /// describing the Rule schema + available connectors, and a user prompt
    /// with the intent. It returns TOML which we parse and validate.
    pub async fn parse(
        adapter: &dyn AiAdapter,
        intent: &str,
        connectors: &[ConnectorInfo],
        options: AiOptions,
    ) -> Result<Rule, AiError> {
        let system_prompt = prompt::build_system_prompt(connectors);
        let user_prompt = prompt::build_user_prompt(intent);

        let request = AiRequest::Chat {
            messages: vec![
                ChatMessage {
                    role: "system".into(),
                    content: system_prompt,
                },
                ChatMessage {
                    role: "user".into(),
                    content: user_prompt,
                },
            ],
        };

        let response = adapter.complete(request, options).await?;

        // Extract TOML from response (may be wrapped in markdown code fences)
        let toml_content = extract_toml(&response.content)?;

        // Parse the TOML into a Rule
        let mut rule: Rule = toml::from_str(&toml_content)
            .map_err(|e| AiError::Serialization(format!("AI generated invalid TOML: {e}")))?;

        // Force Draft status and fresh ID regardless of what the AI generated
        rule.status = RuleStatus::Disabled;
        rule.id = RuleId::new();

        tracing::info!(
            rule_name = %rule.name,
            "NL→Rule: generated draft rule from intent"
        );

        Ok(rule)
    }
}

/// Extract TOML content from an AI response, stripping markdown fences if present.
fn extract_toml(content: &str) -> Result<String, AiError> {
    let trimmed = content.trim();

    // Strip ```toml ... ``` fences
    if let Some(start) = trimmed.find("```toml") {
        let after_fence = &trimmed[start + 7..];
        if let Some(end) = after_fence.find("```") {
            return Ok(after_fence[..end].trim().to_owned());
        }
    }

    // Strip ``` ... ``` fences (no language tag)
    if let Some(start) = trimmed.find("```") {
        let after_fence = &trimmed[start + 3..];
        if let Some(end) = after_fence.find("```") {
            return Ok(after_fence[..end].trim().to_owned());
        }
    }

    // No fences — assume the entire content is TOML
    if trimmed.is_empty() {
        return Err(AiError::Serialization("AI returned empty response".into()));
    }

    Ok(trimmed.to_owned())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_toml_plain() {
        let toml = extract_toml("[rule]\nname = \"test\"").unwrap();
        assert!(toml.contains("[rule]"));
    }

    #[test]
    fn test_extract_toml_fenced() {
        let content = "Here's a rule:\n```toml\n[rule]\nname = \"test\"\n```\nDone.";
        let toml = extract_toml(content).unwrap();
        assert!(toml.contains("[rule]"));
        assert!(!toml.contains("```"));
    }

    #[test]
    fn test_extract_toml_generic_fence() {
        let content = "```\n[rule]\nname = \"test\"\n```";
        let toml = extract_toml(content).unwrap();
        assert!(toml.contains("[rule]"));
    }

    #[test]
    fn test_extract_toml_empty_errors() {
        let result = extract_toml("");
        assert!(result.is_err());
    }
}
