//! `MomentumTier` — Python pyclass facade for the cooperation tier
//! enum (Cold / Warming / Hot / Fever). Conversions live in
//! [`crate::convert`].

use pyo3::prelude::*;

/// Momentum tier — capability gate per `COOPERATION.md §7`. Python sees
/// this as an enum with four members; Rust round-trips through the
/// `MomentumTier::parse` / `Display` pair the rest of the system uses.
#[pyclass(eq, eq_int, frozen)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MomentumTier {
    Cold,
    Warming,
    Hot,
    Fever,
}
