use thiserror::Error;

/// Transport-layer error type.
#[derive(Debug, Error)]
pub enum TransportError {
    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    #[error("message too large: {size} bytes exceeds limit of {limit} bytes")]
    MessageTooLarge { size: usize, limit: usize },

    #[error("message serialization error: {0}")]
    Serialization(String),

    #[error("transport I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("channel closed")]
    ChannelClosed,

    #[error("transport not connected")]
    NotConnected,
}
