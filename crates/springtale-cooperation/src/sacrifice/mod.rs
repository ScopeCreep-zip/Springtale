//! Sacrifice & covering — deliberate self-cost for formation benefit.
//!
//! Per COOPERATION.pdf §24: "Sacrifice is an agent deliberately accepting
//! cost BEFORE failure occurs to benefit the formation."

pub mod action;
pub mod scorer;
mod types;

pub use action::SacrificeAction;
pub use scorer::{evaluate_action, evaluate_sacrifice, FormationSnapshot, SacrificeEvaluation};
pub use types::{SacrificeCost, SacrificeType};
