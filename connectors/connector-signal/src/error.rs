use thiserror::Error;

#[derive(Debug, Error)]
pub enum SignalError {
    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    #[error("send failed: {0}")]
    SendFailed(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("unknown action: {0}")]
    UnknownAction(String),

    #[error("API error: {0}")]
    ApiError(String),

    #[error("daemon unreachable: {0}")]
    DaemonUnreachable(String),
}

impl From<SignalError> for springtale_connector::error::ConnectorError {
    fn from(e: SignalError) -> Self {
        springtale_connector::error::ConnectorError::ExecutionFailed(e.to_string())
    }
}
