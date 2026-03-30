use thiserror::Error;

/// Errors specific to the Bluesky connector.
#[derive(Debug, Error)]
pub enum BlueskyError {
    /// ATProto session/API error.
    #[error("AT Protocol error: {0}")]
    AtProtoError(String),

    /// Jetstream/firehose connection lost.
    #[error("firehose connection lost: {0}")]
    FirehoseDisconnected(String),

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

impl From<BlueskyError> for springtale_connector::error::ConnectorError {
    fn from(e: BlueskyError) -> Self {
        springtale_connector::error::ConnectorError::ExecutionFailed(e.to_string())
    }
}
