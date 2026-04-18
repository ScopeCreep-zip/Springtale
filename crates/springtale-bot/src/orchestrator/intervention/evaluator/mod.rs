//! Evaluator — pure policy from signals to `Intervention`.
//!
//! Isolated from side-effect code in `action/` so the rules are auditable
//! and swappable without touching Formation mutation paths.

pub mod rules;
pub mod thresholds;

pub use rules::RuleBasedEvaluator;
pub use thresholds::InterventionThresholds;
