#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod error;
pub mod http;
pub mod local;
pub mod transport;
pub mod veilid;

pub use error::TransportError;
pub use transport::Message;
pub use transport::Transport;
