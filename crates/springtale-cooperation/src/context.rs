//! FormationContext — shared coordination state watched by all members.
//!
//! Per COOPERATION_IMPLEMENTATION_PLAN.md §5.6: the formation context is
//! shared via `watch::Sender<FormationContext>` so all members see the
//! latest intent, momentum, constraints, and pacing phase without polling.

use crate::cadence::IntentPattern;
use crate::momentum::MomentumTier;
use crate::types::FormationConstraints;

/// Shared coordination state for a formation.
///
/// Distributed to all members via `watch::channel`. Members receive
/// updates automatically when any field changes — no polling needed.
/// This is the "shared mental context" that enables decentralized
/// decision-making per CTDE (Centralized Training, Decentralized Execution).
#[derive(Clone, Debug)]
pub struct FormationContext {
    /// What the formation should accomplish (not how).
    pub intent: IntentPattern,
    /// Current momentum tier — determines available capabilities.
    pub momentum_tier: MomentumTier,
    /// Formation constraints (timeout, max concurrent, guard mode).
    pub constraints: FormationConstraints,
    /// Whether the formation is in guard mode (Total War: don't pursue).
    pub guard_mode: bool,
    /// Number of operational members.
    pub operational_count: usize,
    /// Total member count.
    pub member_count: usize,
    /// Whether the formation is paused (tick processing skipped).
    pub paused: bool,
}

impl Default for FormationContext {
    fn default() -> Self {
        Self {
            intent: IntentPattern::Stabilize {
                reason: "assembling".into(),
            },
            momentum_tier: MomentumTier::Cold,
            constraints: FormationConstraints::default(),
            guard_mode: false,
            operational_count: 0,
            member_count: 0,
            paused: false,
        }
    }
}
