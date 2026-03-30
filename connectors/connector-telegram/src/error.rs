use thiserror::Error;

#[derive(Debug, Error)]
pub enum TelegramError {
    #[error("authentication failed: {0}")]
    AuthFailed(String),

    #[error("API request failed: {0}")]
    RequestFailed(String),

    #[error("unknown action: {0}")]
    UnknownAction(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("unknown trigger: {0}")]
    UnknownTrigger(String),

    #[error("HTTP error: {0}")]
    Reqwest(#[from] reqwest::Error),

    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("polling error: {0}")]
    PollingFailed(String),

    #[error("webhook verification failed")]
    WebhookVerificationFailed,

    #[error("rate limited: retry after {retry_after} seconds")]
    RateLimited { retry_after: u64 },
}

impl From<TelegramError> for springtale_connector::error::ConnectorError {
    fn from(e: TelegramError) -> Self {
        springtale_connector::error::ConnectorError::ExecutionFailed(e.to_string())
    }
}
