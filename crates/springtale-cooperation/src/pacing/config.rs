//! `[cooperation.pacing]` — every number in Booth's Director loop, as
//! configuration rather than a constant.
//!
//! Booth's deck (GDC 2009, slides 79–92) gives the timings; the four
//! stress weights are Springtale's own starting values. They are
//! configuration for the same reason [`crate::momentum::MomentumConfig`]
//! is: Left 4 Dead ships every Director number as a cvar or a
//! `DirectorOptions` field (COOPERATION.md A.1.1), and Total War keeps
//! its morale and fatigue numbers in database tables (A.4.1). Tuning
//! happens after play, not before.
//!
//! [`crate::types::FormationConstraints`] carries one of these, so a
//! formation paces on its own numbers like every other constraint.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use specta::Type;

/// Intensity at which `BuildUp` gives way to `SustainPeak`.
pub const PEAK_THRESHOLD: f32 = 0.6;
/// Booth: "3-5 seconds after Survivor Intensity has peaked."
pub const SUSTAIN_SECS: u64 = 4;
/// Booth: "30-45 seconds, or until Survivors have traveled far enough."
pub const RELAX_SECS: u64 = 35;
/// Booth: "Decay Survivor Intensity towards zero over time."
pub const DECAY_PER_SEC: f32 = 0.05;
/// Booth: "When injured by the Infected, proportional to damage taken."
pub const W_FAILURE: f32 = 0.3;
/// Booth: "When player is pulled/pushed off of a ledge by the Infected."
pub const W_INTERFERENCE: f32 = 0.4;
/// Sentinel `Throttle` verdicts — a nearby threat, not a wound.
pub const W_THROTTLE: f32 = 0.1;
/// Approval denials / quarantines — the formation was stopped.
pub const W_DENIAL: f32 = 0.2;

/// Per-formation pacing numbers: the peak threshold, the two phase
/// timings, the idle decay rate, and the four stress weights.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct PacingConfig {
    /// Intensity at which `BuildUp` hands over to `SustainPeak`.
    pub peak_threshold: f32,
    /// How long `SustainPeak` holds before `PeakFade`.
    pub sustain_secs: u64,
    /// How long `Relax` holds before `BuildUp` resumes.
    pub relax_secs: u64,
    /// Intensity shed per second while the formation is not engaged.
    pub decay_per_sec: f32,
    /// Weight of one failed action.
    pub w_failure: f32,
    /// Weight of one interference event.
    pub w_interference: f32,
    /// Weight of one sentinel `Throttle` verdict.
    pub w_throttle: f32,
    /// Weight of one approval denial or quarantine.
    pub w_denial: f32,
}

impl Default for PacingConfig {
    fn default() -> Self {
        Self {
            peak_threshold: PEAK_THRESHOLD,
            sustain_secs: SUSTAIN_SECS,
            relax_secs: RELAX_SECS,
            decay_per_sec: DECAY_PER_SEC,
            w_failure: W_FAILURE,
            w_interference: W_INTERFERENCE,
            w_throttle: W_THROTTLE,
            w_denial: W_DENIAL,
        }
    }
}

impl PacingConfig {
    /// `sustain_secs` as a `Duration`.
    pub fn sustain(&self) -> Duration {
        Duration::from_secs(self.sustain_secs)
    }

    /// `relax_secs` as a `Duration`.
    pub fn relax(&self) -> Duration {
        Duration::from_secs(self.relax_secs)
    }
}
