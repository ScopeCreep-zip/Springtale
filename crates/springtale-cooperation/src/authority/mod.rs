//! Momentum × layer authority matrix.
//!
//! Encodes spec §7's "trust accumulates, tiers open" semantics as a static
//! table. Each step/trigger in the cooperation pipeline consults this gate
//! before invoking its corresponding layer.

pub mod check;
pub mod matrix;

pub use check::{Unauthorized, allows, require};
