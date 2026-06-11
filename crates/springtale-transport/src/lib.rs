#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod crypto_provider;
pub mod error;
pub mod http;
pub mod local;
pub mod safe_http;
pub mod transport;
pub mod veilid;

pub use error::TransportError;
pub use transport::Message;
pub use transport::Transport;
