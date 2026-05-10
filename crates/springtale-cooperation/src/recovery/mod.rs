//! Recovery & mutual aid — distress detection, recovery delivery.
//!
//! Per COOPERATION.pdf §18: "The other side of cooperation is agents
//! actively seeking out and helping each other."
//!
//! Module layout:
//! - `types` — `DistressSignal`, `RecoveryAction`, `RecoveryCost`,
//!   `ProtectionType`, `FailureMode` enums (§18.1).
//! - `apply` — FSM that steps `AgentHealth` forward when a
//!   `RecoveryAction` lands (§18.2 escalating fragility).
//! - `executor` — local utility-AI evaluator that *picks* the action
//!   to propose (§18.3 decision framework).

pub mod apply;
pub mod executor;
mod types;

pub use apply::{RecoveryKind, MAX_QUICK_FIX_COUNT};
pub use types::{
    DistressSignal, FailureMode, ProtectionType, RecoveryAction, RecoveryCost,
};
