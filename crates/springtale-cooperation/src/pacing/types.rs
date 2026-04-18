use std::time::{Duration, Instant};

/// Current pacing phase for a formation.
///
/// From COOPERATION.md §22.2 — five phases inspired by L4D Director,
/// Total War fatigue, Siege timer, Patapon BPM, DRG swarm waves.
pub enum PacingPhase {
    Preparation { started: Instant },
    Active { intensity: f32, started: Instant },
    Peak { intensity: f32, fuel_rate: f32, started: Instant },
    Recovery { remaining: Duration },
    Disruption { event: String },
}

pub struct PacingTransition {
    pub from: &'static str,
    pub to: &'static str,
}
