//! Pacing system — work/rest cycle management inspired by L4D's AI Director.
//!
//! Per COOPERATION.md §22: effective formations alternate between work peaks
//! and recovery valleys. The `PacingManager` drives phase transitions; the
//! `GovernorRateLimiter` enforces per-phase action throughput via `governor`
//! GCRA (Generic Cell Rate Algorithm).
//!
//! File split:
//! - `types.rs` — phase + transition enums
//! - `manager.rs` — transition logic + rate-limiter composition
//! - `rate_limiter.rs` — governor GCRA wrapper + per-phase quota table

pub mod manager;
pub mod rate_limiter;
pub mod types;

pub use manager::PacingManager;
pub use rate_limiter::GovernorRateLimiter;
pub use types::{PacingPhase, PacingTransition};
