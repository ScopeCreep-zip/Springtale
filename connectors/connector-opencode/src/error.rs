use thiserror::Error;

/// Errors specific to the OpenCode connector.
#[derive(Debug, Error)]
pub enum OpenCodeError {
    /// An API request to the `opencode serve` daemon failed.
    #[error("opencode request failed: {0}")]
    RequestFailed(String),

    /// Invalid input parameters.
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// Configuration is invalid.
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    /// An underlying reqwest error.
    #[error("HTTP error: {0}")]
    Reqwest(#[from] reqwest::Error),
}

impl From<OpenCodeError> for springtale_connector::error::ConnectorError {
    fn from(e: OpenCodeError) -> Self {
        springtale_connector::error::ConnectorError::ExecutionFailed(e.to_string())
    }
}
