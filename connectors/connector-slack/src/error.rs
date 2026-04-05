use thiserror::Error;

#[derive(Debug, Error)]
pub enum SlackError {
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

    #[error("WebSocket error: {0}")]
    WebSocketError(String),
}

impl From<SlackError> for springtale_connector::error::ConnectorError {
    fn from(e: SlackError) -> Self {
        springtale_connector::error::ConnectorError::ExecutionFailed(e.to_string())
    }
}
