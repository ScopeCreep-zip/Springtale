//! `From` conversions between the Python-facing pyclass types and
//! their internal `springtale_cooperation` equivalents. Kept in their
//! own module so the type definitions stay focused on the pyo3
//! decorations and method surface.

use springtale_cooperation::momentum::MomentumTier as CoreTier;

use crate::momentum::MomentumTier;

impl From<CoreTier> for MomentumTier {
    fn from(t: CoreTier) -> Self {
        match t {
            CoreTier::Cold => Self::Cold,
            CoreTier::Warming => Self::Warming,
            CoreTier::Hot => Self::Hot,
            CoreTier::Fever => Self::Fever,
        }
    }
}

impl From<MomentumTier> for CoreTier {
    fn from(t: MomentumTier) -> Self {
        match t {
            MomentumTier::Cold => CoreTier::Cold,
            MomentumTier::Warming => CoreTier::Warming,
            MomentumTier::Hot => CoreTier::Hot,
            MomentumTier::Fever => CoreTier::Fever,
        }
    }
}
