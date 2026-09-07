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

// `approval::AutoAllowApprovalGate` is deliberately NOT re-exported: it
// is `#[cfg(test)]`-only so no production caller of
// `Sentinel::with_approval_gate` can disable the human approval gate.
pub use approval::{
    ApprovalGate, ApprovalRequest, ChannelApprovalGate, DefaultDenyApprovalGate, PendingApproval,
};
pub use config::SentinelConfig;
pub use error::SentinelError;
pub use impact::{ActionHints, ActionImpact};
pub use sentinel::{EvaluateRequest, Sentinel};
pub use throttle_tier::ThrottleTier;
pub use verdict::Verdict;
