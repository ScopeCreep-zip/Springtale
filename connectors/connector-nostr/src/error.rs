use thiserror::Error;

#[derive(Debug, Error)]
pub enum NostrError {
    #[error("key error: {0}")]
    KeyError(String),

    #[error("relay error: {0}")]
    RelayError(String),

    #[error("publish failed: {0}")]
    PublishFailed(String),

    #[error("encryption error: {0}")]
    EncryptionError(String),

    #[error("unknown action: {0}")]
    UnknownAction(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("unknown trigger: {0}")]
    UnknownTrigger(String),

    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
}

impl From<NostrError> for springtale_connector::error::ConnectorError {
    fn from(e: NostrError) -> Self {
        springtale_connector::error::ConnectorError::ExecutionFailed(e.to_string())
    }
}
