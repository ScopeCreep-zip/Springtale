//! Sacrifice & covering — deliberate self-cost for formation benefit.
//!
//! Per COOPERATION.pdf §24: "Sacrifice is an agent deliberately accepting
//! cost BEFORE failure occurs to benefit the formation."

pub mod evaluator;
mod types;

pub use types::{SacrificeCost, SacrificeType};
