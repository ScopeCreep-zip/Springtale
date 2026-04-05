use thiserror::Error;

#[derive(Debug, Error)]
pub enum IrcError {
    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    #[error("auth failed: {0}")]
    AuthFailed(String),

    #[error("send failed: {0}")]
    SendFailed(String),

    #[error("unknown action: {0}")]
    UnknownAction(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("unknown trigger: {0}")]
    UnknownTrigger(String),

    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
}

impl From<IrcError> for springtale_connector::error::ConnectorError {
    fn from(e: IrcError) -> Self {
        springtale_connector::error::ConnectorError::ExecutionFailed(e.to_string())
    }
}
