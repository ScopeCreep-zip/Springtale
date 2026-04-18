//! L6 commander override — kicks in only when cooperation primitives exhaust.
//!
//! Layered after §15 rally (cooperation), §18 recovery (cooperation), and
//! §L5 CBBA replan (cooperation) have all failed. See COOPERATION.md §3.4.
//!
//! The split:
//! - `types.rs` — the `Intervention` enum (commander verbs)
//! - `evaluator/` — pure signal-to-intervention policy
//! - `action/` — side-effect executors, one file per intervention kind
//! - `trait_.rs` — trait seams so bots can swap evaluators for tests

pub mod action;
pub mod evaluator;
pub mod trait_;
pub mod types;

pub use trait_::{InterventionAction, InterventionEvaluator};
pub use types::{Intervention, InterventionError, InterventionSignals};
