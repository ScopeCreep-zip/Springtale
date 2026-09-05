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

    /// An error surfaced by the octocrab GitHub SDK (transport, HTTP status,
    /// or response decoding).
    #[error("GitHub API error: {0}")]
    Api(#[from] octocrab::Error),

    /// A git ref resolved to something other than a commit (e.g. an
    /// annotated tag), so it cannot be used as a branch base.
    #[error("git ref does not point at a commit")]
    UnexpectedRef,

    /// Configuration is invalid.
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
}

impl From<GithubError> for springtale_connector::error::ConnectorError {
    fn from(e: GithubError) -> Self {
        springtale_connector::error::ConnectorError::ExecutionFailed(e.to_string())
    }
}
