use thiserror::Error;

/// Errors specific to the HTTP connector.
#[derive(Debug, Error)]
pub enum HttpError {
    /// An HTTP request failed.
    #[error("request failed: {0}")]
    RequestFailed(String),

    /// The response body could not be parsed.
    #[error("response parse error: {0}")]
    ParseError(String),

    /// The target host is not in the allow-list.
    #[error("host not allowed: {0}")]
    HostNotAllowed(String),

    /// The URL could not be parsed.
    #[error("invalid URL: {0}")]
    InvalidUrl(String),

    /// The action name is not recognized.
    #[error("unknown action: {0}")]
    UnknownAction(String),

    /// Invalid input parameters for an action.
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// An underlying reqwest error.
    #[error("HTTP client error: {0}")]
    Reqwest(#[from] reqwest::Error),

    /// Configuration is invalid.
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
}

impl From<HttpError> for springtale_connector::error::ConnectorError {
    fn from(e: HttpError) -> Self {
        springtale_connector::error::ConnectorError::ExecutionFailed(e.to_string())
    }
}
