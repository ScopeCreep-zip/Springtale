use thiserror::Error;

/// Top-level error type for springtale-bot.
#[derive(Debug, Error)]
pub enum BotError {
    #[error("handler error: {0}")]
    Handler(String),

    #[error("router error: {0}")]
    Router(String),

    #[error("session error: {0}")]
    Session(String),

    #[error("memory error: {0}")]
    Memory(String),

    #[error("storage error: {0}")]
    Storage(#[from] springtale_store::StoreError),

    #[error("connector error: {0}")]
    Connector(#[from] springtale_connector::error::ConnectorError),

    #[error("crypto error: {0}")]
    Crypto(#[from] springtale_crypto::error::CryptoError),

    #[error("command not found: {0}")]
    CommandNotFound(String),

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("bot not initialized: {0}")]
    NotInitialized(String),
}
