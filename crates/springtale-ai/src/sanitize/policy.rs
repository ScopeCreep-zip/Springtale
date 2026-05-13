use serde::{Deserialize, Serialize};

use specta::Type;
/// How the sanitizer handles detected sensitive patterns.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum SanitizePolicy {
    /// Log a warning but allow the request through. Default.
    #[default]
    Warn,
    /// Replace detected content with `[REDACTED]`.
    Redact,
    /// Block the request entirely (return AiError::SanitizationBlocked).
    Block,
}

/// What category of sensitive pattern was detected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatternType {
    /// Personally identifiable information (SSN, credit card, phone, email).
    Pii,
    /// Credentials (API keys, tokens, passwords, secrets).
    Credential,
    /// Prompt injection attempt ("ignore previous instructions", etc.).
    PromptInjection,
    /// Content exceeds maximum length.
    ContentTooLong,
    /// Suspicious encoding (base64/hex blobs that may hide data).
    SuspiciousEncoding,
}

impl std::fmt::Display for PatternType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PatternType::Pii => write!(f, "PII"),
            PatternType::Credential => write!(f, "credential"),
            PatternType::PromptInjection => write!(f, "prompt_injection"),
            PatternType::ContentTooLong => write!(f, "content_too_long"),
            PatternType::SuspiciousEncoding => write!(f, "suspicious_encoding"),
        }
    }
}

/// A warning from the sanitizer about detected sensitive content.
#[derive(Debug, Clone)]
pub struct SanitizeWarning {
    /// Which field contained the match (e.g., "prompt", "messages[0].content").
    pub field: String,
    /// What kind of pattern was detected.
    pub pattern_type: PatternType,
    /// Human-readable detail about the match.
    pub detail: String,
}

/// Result of sanitizing an AiRequest.
#[derive(Debug)]
pub struct SanitizeResult {
    /// The (possibly redacted) request text.
    pub text: String,
    /// Warnings about detected patterns.
    pub warnings: Vec<SanitizeWarning>,
    /// Whether the request should be blocked.
    pub blocked: bool,
}
