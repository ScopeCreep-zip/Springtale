use regex::Regex;

use super::policy::PatternType;

/// A compiled pattern for detecting sensitive content.
pub struct SensitivePattern {
    pub pattern_type: PatternType,
    pub regex: Regex,
    pub description: &'static str,
}

/// Build the default set of sensitive content patterns.
///
/// Based on OWASP LLM Prompt Injection Prevention Cheat Sheet and
/// OWASP AI Agent Security Cheat Sheet recommendations:
/// - PII patterns (SSN, credit card, phone, email)
/// - Credential patterns (API keys, tokens, passwords)
/// - Prompt injection patterns ("ignore previous instructions", etc.)
/// - Suspicious encoding patterns (base64 blobs)
pub fn default_patterns() -> Vec<SensitivePattern> {
    let definitions: &[(&str, PatternType, &str)] = &[
        // ── PII ─────────────────────────────────────────────────
        (
            r"\b\d{3}-\d{2}-\d{4}\b",
            PatternType::Pii,
            "US Social Security Number (XXX-XX-XXXX)",
        ),
        (
            r"\b\d{4}[\s-]?\d{4}[\s-]?\d{4}[\s-]?\d{4}\b",
            PatternType::Pii,
            "credit card number (16 digits)",
        ),
        (
            r"\b\d{3}[\s.-]?\d{3}[\s.-]?\d{4}\b",
            PatternType::Pii,
            "US phone number (XXX-XXX-XXXX)",
        ),
        (
            r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b",
            PatternType::Pii,
            "email address",
        ),
        // ── Credentials ─────────────────────────────────────────
        (
            r"(?i)(api[_\s-]?key|api[_\s-]?token|auth[_\s-]?token)\s*[:=]\s*\S+",
            PatternType::Credential,
            "API key or token assignment",
        ),
        (
            r"(?i)(password|passwd|secret)\s*[:=]\s*\S+",
            PatternType::Credential,
            "password or secret assignment",
        ),
        (
            r"(?i)bearer\s+[a-zA-Z0-9._~+/=-]{20,}",
            PatternType::Credential,
            "bearer token",
        ),
        (
            r"(?i)(sk|pk)[-_][a-zA-Z0-9]{20,}",
            PatternType::Credential,
            "API secret/public key (sk-/pk- prefix)",
        ),
        // ── Prompt Injection (per OWASP LLM Cheat Sheet) ────────
        (
            r"(?i)ignore\s+(all\s+)?previous\s+instructions?",
            PatternType::PromptInjection,
            "prompt injection: ignore previous instructions",
        ),
        (
            r"(?i)you\s+are\s+now\s+(in\s+)?developer\s+mode",
            PatternType::PromptInjection,
            "prompt injection: developer mode claim",
        ),
        (
            r"(?i)system\s+override",
            PatternType::PromptInjection,
            "prompt injection: system override",
        ),
        (
            r"(?i)reveal\s+(your\s+)?(system\s+)?prompt",
            PatternType::PromptInjection,
            "prompt injection: reveal prompt",
        ),
        (
            r"(?i)disregard\s+(all\s+)?(prior|previous|above)\s+",
            PatternType::PromptInjection,
            "prompt injection: disregard prior context",
        ),
        // ── Suspicious Encoding ─────────────────────────────────
        (
            r"[A-Za-z0-9+/]{40,}={0,2}",
            PatternType::SuspiciousEncoding,
            "possible base64-encoded blob (40+ chars)",
        ),
    ];

    definitions
        .iter()
        .filter_map(|(pattern, ptype, desc)| {
            match Regex::new(pattern) {
                Ok(regex) => Some(SensitivePattern {
                    pattern_type: ptype.clone(),
                    regex,
                    description: desc,
                }),
                Err(e) => {
                    tracing::error!(pattern = pattern, error = %e, "failed to compile sanitize pattern");
                    None
                }
            }
        })
        .collect()
}

/// Maximum allowed content length per field (characters).
pub const MAX_CONTENT_LENGTH: usize = 10_000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_patterns_compile() {
        let patterns = default_patterns();
        assert!(!patterns.is_empty());
        // All patterns should be valid regex
        for p in &patterns {
            assert!(!p.description.is_empty());
        }
    }

    #[test]
    fn test_ssn_pattern_matches() {
        let patterns = default_patterns();
        let ssn_pattern = patterns
            .iter()
            .find(|p| p.description.contains("Social Security"))
            .unwrap();
        assert!(ssn_pattern.regex.is_match("My SSN is 123-45-6789"));
        assert!(!ssn_pattern.regex.is_match("My number is 12345"));
    }

    #[test]
    fn test_credit_card_pattern_matches() {
        let patterns = default_patterns();
        let cc_pattern = patterns
            .iter()
            .find(|p| p.description.contains("credit card"))
            .unwrap();
        assert!(cc_pattern.regex.is_match("Card: 4111 1111 1111 1111"));
        assert!(cc_pattern.regex.is_match("Card: 4111111111111111"));
    }

    #[test]
    fn test_api_key_pattern_matches() {
        let patterns = default_patterns();
        let key_pattern = patterns
            .iter()
            .find(|p| p.description.contains("API key"))
            .unwrap();
        assert!(key_pattern.regex.is_match("api_key: sk-abc123def456"));
        assert!(key_pattern.regex.is_match("API_TOKEN = mytoken123"));
    }

    #[test]
    fn test_prompt_injection_pattern_matches() {
        let patterns = default_patterns();
        let injection = patterns
            .iter()
            .find(|p| p.description.contains("ignore previous"))
            .unwrap();
        assert!(
            injection
                .regex
                .is_match("Please ignore all previous instructions")
        );
        assert!(injection.regex.is_match("IGNORE PREVIOUS INSTRUCTIONS"));
        assert!(!injection.regex.is_match("I want to search for weather"));
    }

    #[test]
    fn test_bearer_token_pattern_matches() {
        let patterns = default_patterns();
        let bearer = patterns
            .iter()
            .find(|p| p.description.contains("bearer"))
            .unwrap();
        assert!(
            bearer
                .regex
                .is_match("Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9")
        );
        assert!(!bearer.regex.is_match("The bear went over the mountain"));
    }

    #[test]
    fn test_base64_pattern_matches() {
        let patterns = default_patterns();
        let b64 = patterns
            .iter()
            .find(|p| p.description.contains("base64"))
            .unwrap();
        assert!(
            b64.regex
                .is_match("SGVsbG8gV29ybGQhIFRoaXMgaXMgYSBiYXNlNjQgdGVzdA==")
        );
        assert!(!b64.regex.is_match("short"));
    }
}
