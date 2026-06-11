use std::time::{Duration, Instant};

/// Current pacing phase for a formation.
///
/// From COOPERATION.md §22.2 — five phases inspired by L4D Director,
/// Total War fatigue, Siege timer, Patapon BPM, DRG swarm waves.
///
/// L4D Director phase mapping (E12 audit-fix). Valve's L4D AI Director
/// names its four phases: **build-up → sustained-peak → peak-fade →
/// relax**. Springtale's names diverge to track what the formation is
/// doing rather than what the Director is doing to it:
///
/// | L4D                 | Springtale                          |
/// |---------------------|-------------------------------------|
/// | build-up            | `Preparation`                       |
/// | sustained-peak      | `Active`                            |
/// | sustained-peak (++) | `Peak` (fuel + intensity scaled up) |
/// | peak-fade           | (folded into `Recovery` head)       |
/// | relax               | `Recovery`                          |
/// | (interrupt)         | `Disruption`                        |
///
/// L4D's peak-fade is a directorial deceleration cue; Springtale models
/// the same easing as the head of `Recovery` (rate-limiter quotas drop
/// before tier transitions cycle back to `Cold`). `Disruption` is the
/// added interrupt phase that has no L4D analogue — it's the
/// "abort-and-cool" path triggered by sentinel-detected anomalies.
pub enum PacingPhase {
    Preparation {
        started: Instant,
    },
    Active {
        intensity: f32,
        started: Instant,
    },
    Peak {
        intensity: f32,
        fuel_rate: f32,
        started: Instant,
    },
    Recovery {
        remaining: Duration,
    },
    Disruption {
        event: String,
    },
}

pub struct PacingTransition {
    pub from: &'static str,
    pub to: &'static str,
}
