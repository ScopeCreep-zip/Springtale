//! Interference detection — conflict between cooperating agents.
//!
//! Per COOPERATION.pdf §13:
//! Game sources: Helldivers 2 friendly fire, Divinity combos hitting
//! allies, Total War archers hitting own infantry.

pub mod detector;
mod types;

pub use types::{ActionRecord, InterferenceEvent, InterferenceType, SideEffect};
