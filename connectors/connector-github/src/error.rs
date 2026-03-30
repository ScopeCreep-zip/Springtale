use thiserror::Error;

/// Errors specific to the GitHub connector.
#[derive(Debug, Error)]
pub enum GithubError {
    /// An API request failed.
    #[error("API request failed: {0}")]
    RequestFailed(String),

    /// Webhook HMAC-SHA256 signature verification failed.
    #[error("webhook signature verification failed")]
    WebhookSignatureInvalid,

    /// The action name is not recognized.
    #[error("unknown action: {0}")]
    UnknownAction(String),

    /// Invalid input parameters.
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// Unknown trigger.
    #[error("unknown trigger: {0}")]
    UnknownTrigger(String),

    /// An underlying reqwest error.
    #[error("HTTP error: {0}")]
    Reqwest(#[from] reqwest::Error),

    /// Configuration is invalid.
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
}

impl From<GithubError> for springtale_connector::error::ConnectorError {
    fn from(e: GithubError) -> Self {
        springtale_connector::error::ConnectorError::ExecutionFailed(e.to_string())
    }
}
