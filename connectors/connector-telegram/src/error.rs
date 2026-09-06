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

    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("polling error: {0}")]
    PollingFailed(String),

    #[error("webhook verification failed")]
    WebhookVerificationFailed,

    #[error("rate limited: retry after {retry_after} seconds")]
    RateLimited { retry_after: u64 },
}

/// Map teloxide-core's request errors onto the connector's typed errors.
///
/// `RetryAfter` becomes [`TelegramError::RateLimited`] so the polling loop
/// keeps its back-off behaviour. teloxide-core strips the bot token from
/// `Network` errors (`hide_token`) before they reach us, so `Display` is
/// safe to surface.
impl From<teloxide_core::RequestError> for TelegramError {
    fn from(e: teloxide_core::RequestError) -> Self {
        match e {
            teloxide_core::RequestError::RetryAfter(secs) => TelegramError::RateLimited {
                retry_after: u64::from(secs.seconds()),
            },
            other => TelegramError::RequestFailed(other.to_string()),
        }
    }
}

impl From<TelegramError> for springtale_connector::error::ConnectorError {
    fn from(e: TelegramError) -> Self {
        springtale_connector::error::ConnectorError::ExecutionFailed(e.to_string())
    }
}
