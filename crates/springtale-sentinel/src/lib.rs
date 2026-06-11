#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod approval;
pub mod audit;
pub mod circuit_breaker;
pub mod config;
pub mod dead_man;
pub mod error;
pub mod impact;
pub mod rate_limiter;
pub mod sentinel;
pub mod throttle_tier;
pub mod toxic_pairs;
pub mod verdict;

pub use approval::{
    ApprovalGate, ApprovalRequest, AutoAllowApprovalGate, ChannelApprovalGate,
    DefaultDenyApprovalGate, PendingApproval,
};
pub use config::SentinelConfig;
pub use error::SentinelError;
pub use sentinel::Sentinel;
pub use throttle_tier::ThrottleTier;
pub use verdict::Verdict;
