use thiserror::Error;

#[derive(Debug, Error)]
pub enum DiscordError {
    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    #[error("auth failed: {0}")]
    AuthFailed(String),

    #[error("send failed: {0}")]
    SendFailed(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("unknown action: {0}")]
    UnknownAction(String),

    #[error("API error: {0}")]
    ApiError(String),
}

impl From<DiscordError> for springtale_connector::error::ConnectorError {
    fn from(e: DiscordError) -> Self {
        springtale_connector::error::ConnectorError::ExecutionFailed(e.to_string())
    }
}
