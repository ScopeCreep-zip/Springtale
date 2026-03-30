#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod adapter;
pub mod error;
pub mod noop;
pub mod sanitize;

pub use adapter::{
    AiAdapter, AiOptions, AiRequest, AiResponse, AiStream, ChatMessage, ConnectorInfo,
    DisclosureLevel, StreamChunk,
};
pub use error::AiError;
pub use noop::NoopAdapter;
pub use sanitize::{SanitizePolicy, Sanitizer};
