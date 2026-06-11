use thiserror::Error;

/// Error type for the AI adapter layer.
#[derive(Debug, Error)]
pub enum AiError {
    /// AI adapter is not configured — NoopAdapter is active.
    #[error("AI adapter disabled (NoopAdapter active)")]
    Disabled,

    /// AI adapter is configured but the endpoint is not reachable.
    #[error("AI adapter not configured: {0}")]
    NotConfigured(String),

    /// Inference call failed.
    #[error("inference failed: {0}")]
    InferenceFailed(String),

    /// Streaming error during response consumption.
    #[error("stream error: {0}")]
    StreamError(String),

    /// Request timed out.
    #[error("AI request timed out")]
    Timeout,

    /// Response exceeded the maximum size limit.
    #[error("AI response too large: {size} bytes exceeds {limit} byte limit")]
    ResponseTooLarge { size: usize, limit: usize },

    /// Serialization/deserialization error.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Input sanitization blocked the request.
    #[error("request blocked by sanitization: {reason}")]
    SanitizationBlocked { reason: String },

    /// The locally-installed model's SHA-256 manifest digest does not
    /// match the user-pinned value. The model store was swapped or
    /// upgraded out from under the audited config — refuse to use it.
    #[error("model digest mismatch for {model}: expected {expected}, observed {observed}")]
    ModelDigestMismatch {
        model: String,
        expected: String,
        observed: String,
    },

    /// Per-bot daily token quota exhausted. Caps the OWASP LLM10
    /// "Unbounded Consumption" surface — a single bot can no longer
    /// stampede the model provider (or the user's metered API plan).
    #[error("ai token quota exceeded for {agent_id}: used {used}, daily limit {limit}")]
    QuotaExceeded {
        agent_id: String,
        used: u64,
        limit: u64,
    },

    /// Quota backend (SQLite, in-memory, etc.) reported an error.
    /// Distinct from `QuotaExceeded` so callers can tell "user is
    /// over their cap" apart from "the store layer is broken."
    #[error("ai quota backend error: {0}")]
    QuotaBackend(String),
}
