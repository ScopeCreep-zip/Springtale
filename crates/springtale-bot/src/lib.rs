#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod bridge;
pub mod colony;
pub mod conversation;
pub mod cooperation;
pub mod error;
pub mod handler;
pub mod identity;
pub mod memory;
pub mod orchestrator;
pub mod router;
pub mod runtime;
pub mod state;
pub mod tool_runner;

pub use error::BotError;
pub use router::RouteResult;
pub use runtime::lifecycle::{Bot, BotBuilder, OutgoingResponse};
