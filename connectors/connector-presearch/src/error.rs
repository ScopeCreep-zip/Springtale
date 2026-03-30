use thiserror::Error;

/// Errors specific to the Presearch connector.
#[derive(Debug, Error)]
pub enum PresearchError {
    /// A search query failed.
    #[error("search query failed: {0}")]
    QueryFailed(String),

    /// The action name is not recognized.
    #[error("unknown action: {0}")]
    UnknownAction(String),

    /// Invalid input parameters.
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// An underlying reqwest error.
    #[error("HTTP error: {0}")]
    Reqwest(#[from] reqwest::Error),

    /// Configuration is invalid.
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
}

impl From<PresearchError> for springtale_connector::error::ConnectorError {
    fn from(e: PresearchError) -> Self {
        springtale_connector::error::ConnectorError::ExecutionFailed(e.to_string())
    }
}
