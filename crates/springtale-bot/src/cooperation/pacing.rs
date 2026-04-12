//! Pacing system — work/rest cycle management inspired by L4D's AI Director.
//!
//! Per COOPERATION.pdf §22:
//! "Constant high intensity burns agents out. Constant idleness wastes capacity.
//! Effective formations alternate between work peaks and recovery valleys."
//!
//! Game patterns:
//! - L4D Director: Adaptive Dramatic Pacing (peaks and valleys)
//! - Total War: Fatigue-enforced rest cycles
//! - Siege: Timer-enforced acceleration (3-minute rounds)
//! - Patapon: BPM as pacing (music tempo controls speed)
//! - Overcooked: Disruptions as pacing resets
//! - Helldivers: Extraction as pacing climax
//! - MH: Monster behavior as pacing (attack → exhaust → enrage)
//! - DRG: Swarm waves as pacing

use std::time::{Duration, Instant};

/// Current pacing phase for a formation.
///
/// From COOPERATION.pdf §22.2:
pub enum PacingPhase {
    /// Low intensity. Information gathering, preparation.
    /// L4D: quiet traversal. Siege: drone phase. DRG: exploration.
    Preparation { started: Instant },

    /// Building intensity. Work in progress, some pressure.
    /// MH: engaging the monster. Overcooked: orders coming in.
    Active { intensity: f32, started: Instant },

    /// Peak intensity. Maximum output, high fuel consumption.
    /// L4D: Tank encounter. Helldivers: extraction. Siege: execute.
    Peak {
        intensity: f32,
        fuel_rate: f32,
        started: Instant,
    },

    /// Mandatory rest. Recovery, consolidation.
    /// Total War: fatigued units resting. L4D: safe room. DRG: resupply convergence.
    Recovery { remaining: Duration },

    /// Disruption. External event forces re-coordination.
    /// Overcooked: kitchen shift. MH: monster area transition.
    Disruption { event: String },
}

/// Manages pacing for a formation — L4D Director-inspired.
///
/// From COOPERATION.pdf §22.2:
pub struct PacingManager {
    pub current_phase: PacingPhase,
    pub cumulative_intensity: f32,
    pub time_since_last_recovery: Duration,
    pub disruption_count: u32,

    // L4D Director-inspired thresholds
    pub peak_duration_max: Duration,
    pub recovery_duration_min: Duration,
    pub intensity_ceiling: f32,
}

impl Default for PacingManager {
    fn default() -> Self {
        Self {
            current_phase: PacingPhase::Preparation {
                started: Instant::now(),
            },
            cumulative_intensity: 0.0,
            time_since_last_recovery: Duration::ZERO,
            disruption_count: 0,
            peak_duration_max: Duration::from_secs(120),
            recovery_duration_min: Duration::from_secs(30),
            intensity_ceiling: 0.9,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_pacing_default() {
        let pm = PacingManager::default();
        assert!(matches!(pm.current_phase, PacingPhase::Preparation { .. }));
        assert_eq!(pm.cumulative_intensity, 0.0);
        assert_eq!(pm.disruption_count, 0);
    }

    #[test]
    fn test_pacing_phases() {
        let _prep = PacingPhase::Preparation {
            started: Instant::now(),
        };
        let _active = PacingPhase::Active {
            intensity: 0.5,
            started: Instant::now(),
        };
        let _peak = PacingPhase::Peak {
            intensity: 0.9,
            fuel_rate: 2.0,
            started: Instant::now(),
        };
        let _recovery = PacingPhase::Recovery {
            remaining: Duration::from_secs(30),
        };
        let _disruption = PacingPhase::Disruption {
            event: "connector_timeout".into(),
        };
    }
}
