//! Cooperation error hierarchy — per-module sub-errors composed into a
//! cross-module `CooperationError` aggregate via `#[from]`.
//!
//! Per COOPERATION_IMPLEMENTATION_PLAN.md §5.1 and Phase A (gap G1): narrow
//! error types let each module's API say what can actually go wrong; the
//! aggregate stays available for cross-module boundaries (e.g. the
//! `LayerOutcome::Failed` channel). Error IDs (`COOP-XXXX`) are stable
//! across the split.

pub mod aggregate;
pub mod awareness;
pub mod cadence;
pub mod commit;
pub mod consensus;
pub mod formation;
pub mod handoff;
pub mod interference;
pub mod momentum;
pub mod pacing;
pub mod rally;
pub mod recovery;

pub use aggregate::CooperationError;
pub use awareness::AwarenessError;
pub use cadence::CadenceError;
pub use commit::CommitError;
pub use consensus::ConsensusError;
pub use formation::FormationError;
pub use handoff::HandoffError;
pub use interference::InterferenceError;
pub use momentum::MomentumError;
pub use pacing::PacingError;
pub use rally::RallyError;
pub use recovery::RecoveryError;
