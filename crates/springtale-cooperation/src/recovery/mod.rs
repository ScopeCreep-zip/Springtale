//! Recovery & mutual aid — distress detection, recovery delivery.
//!
//! Per COOPERATION.pdf §18: "The other side of cooperation is agents
//! actively seeking out and helping each other."

pub mod executor;
mod types;

pub use types::{
    DistressSignal, FailureMode, ProtectionType, RecoveryAction, RecoveryCost,
};
