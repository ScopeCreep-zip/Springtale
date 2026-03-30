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
}
