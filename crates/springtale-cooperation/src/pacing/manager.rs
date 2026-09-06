//! PacingManager — intensity is stress; at peak, back off; frequency
//! changes, amplitude never does (Booth, GDC 2009, slides 79–92).

use std::time::{Duration, Instant};

use super::types::{PacingPhase, PacingTransition};

/// Intensity at which `BuildUp` gives way to `SustainPeak`.
pub const PEAK_THRESHOLD: f32 = 0.6;
/// Booth: "3-5 seconds after Survivor Intensity has peaked."
pub const SUSTAIN: Duration = Duration::from_secs(4);
/// Booth: "30-45 seconds, or until Survivors have traveled far enough."
pub const RELAX: Duration = Duration::from_secs(35);
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

/// One tick's stress inputs. Booth's increase rules (slide 80) mapped to
/// a bot formation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StressSample {
    /// "injured... proportional to damage taken"
    pub failures: u32,
    /// "pulled/pushed off of a ledge by the Infected"
    pub interferences: u32,
    /// Sentinel `Throttle` verdicts this tick.
    pub throttles: u32,
    /// Approval denials / quarantines this tick.
    pub denials: u32,
    pub members: u32,
    /// Any action in flight: "Do NOT decay... if actively engaging."
    pub engaged: bool,
}

/// Manages pacing for a formation.
pub struct PacingManager {
    pub current_phase: PacingPhase,
    /// Booth's Survivor Intensity, 0.0–1.0. Stress, not work done.
    pub intensity: f32,
    pub disruption_count: u32,
    /// Formation time: advances by the elapsed duration of each observed
    /// tick, so phase timers are deterministic under the tick divider.
    clock: Instant,
}

impl Default for PacingManager {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            current_phase: PacingPhase::BuildUp { started: now },
            intensity: 0.0,
            disruption_count: 0,
            clock: now,
        }
    }
}

impl PacingManager {
    /// Fold one tick's stress into intensity and advance the phase
    /// machine. `elapsed` is wall-clock time since the previous
    /// observed tick.
    pub fn observe(&mut self, s: &StressSample, elapsed: Duration) -> Option<PacingTransition> {
        let per_member = s.members.max(1) as f32;
        let harm = (W_FAILURE * s.failures as f32
            + W_INTERFERENCE * s.interferences as f32
            + W_THROTTLE * s.throttles as f32
            + W_DENIAL * s.denials as f32)
            / per_member;
        self.intensity = (self.intensity + harm).min(1.0);
        if !s.engaged {
            self.intensity = (self.intensity - DECAY_PER_SEC * elapsed.as_secs_f32()).max(0.0);
        }
        self.clock += elapsed;
        let now = self.clock;
        let next = match &self.current_phase {
            PacingPhase::BuildUp { .. } if self.intensity >= PEAK_THRESHOLD => {
                Some(PacingPhase::SustainPeak { peaked_at: now })
            }
            PacingPhase::SustainPeak { peaked_at } if now.duration_since(*peaked_at) >= SUSTAIN => {
                Some(PacingPhase::PeakFade { since: now })
            }
            // Booth: "Peak Fade won't allow the Relax period to start
            // until a natural break in the action occurs."
            PacingPhase::PeakFade { .. } if !s.engaged || self.intensity < PEAK_THRESHOLD => {
                Some(PacingPhase::Relax { until: now + RELAX })
            }
            PacingPhase::Relax { until } if now >= *until => {
                Some(PacingPhase::BuildUp { started: now })
            }
            PacingPhase::Disruption { .. } => Some(PacingPhase::BuildUp { started: now }),
            _ => None,
        };
        next.map(|p| self.set_phase(p))
    }

    /// Frequency only. Booth: "Amplitude (difficulty) is not changed,
    /// frequency (pacing) is." One CadenceBus serves every formation, so
    /// a formation processes only bus ticks where
    /// `sequence % divider == 0`.
    pub fn tick_divider(&self) -> u64 {
        match self.current_phase {
            PacingPhase::Relax { .. } => 4,
            PacingPhase::PeakFade { .. } => 2,
            _ => 1,
        }
    }

    /// In Relax the formation senses but does not act. Everything else
    /// is unthrottled here; per-connector rate limits stay in the
    /// sentinel.
    pub fn allows(&self, read_only: bool) -> bool {
        !matches!(self.current_phase, PacingPhase::Relax { .. }) || read_only
    }

    pub fn phase_name(&self) -> &'static str {
        match &self.current_phase {
            PacingPhase::BuildUp { .. } => "BuildUp",
            PacingPhase::SustainPeak { .. } => "SustainPeak",
            PacingPhase::PeakFade { .. } => "PeakFade",
            PacingPhase::Relax { .. } => "Relax",
            PacingPhase::Disruption { .. } => "Disruption",
        }
    }

    pub fn disrupt(&mut self, event: String) {
        self.disruption_count += 1;
        self.set_phase(PacingPhase::Disruption { event });
    }

    fn set_phase(&mut self, phase: PacingPhase) -> PacingTransition {
        let from = self.phase_name();
        self.current_phase = phase;
        PacingTransition {
            from,
            to: self.phase_name(),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    const TICK: Duration = Duration::from_millis(33);

    fn ok(members: u32) -> StressSample {
        StressSample {
            members,
            engaged: true,
            ..StressSample::default()
        }
    }

    fn failing(members: u32, failures: u32) -> StressSample {
        StressSample {
            failures,
            ..ok(members)
        }
    }

    #[test]
    fn test_observe_fifty_successful_engaged_ticks_stays_in_build_up() {
        let mut m = PacingManager::default();
        for _ in 0..50 {
            assert!(m.observe(&ok(3), TICK).is_none());
        }
        assert!(matches!(m.current_phase, PacingPhase::BuildUp { .. }));
        assert_eq!(m.intensity, 0.0);
    }

    #[test]
    fn test_observe_failures_reach_sustain_peak_then_fade_then_relax_then_build_up() {
        let mut m = PacingManager::default();
        let mut transitions = Vec::new();
        for _ in 0..5 {
            transitions.extend(m.observe(&failing(2, 2), TICK));
        }
        assert!(matches!(m.current_phase, PacingPhase::SustainPeak { .. }));
        assert_eq!(transitions.len(), 1);
        assert_eq!(transitions[0].to, "SustainPeak");
        // Still engaged, still stressed: sustain holds for SUSTAIN.
        assert!(m.observe(&failing(2, 2), TICK).is_none());
        assert_eq!(m.tick_divider(), 1);
        let t = m.observe(&ok(2), SUSTAIN).expect("sustain elapsed");
        assert_eq!((t.from, t.to), ("SustainPeak", "PeakFade"));
        assert_eq!(m.tick_divider(), 2);
        // Peak fade waits for a natural break: not engaged.
        let t = m.observe(&StressSample::default(), TICK).expect("break");
        assert_eq!((t.from, t.to), ("PeakFade", "Relax"));
        assert_eq!(m.tick_divider(), 4);
        // Relax returns to BuildUp once the relax period elapses.
        assert!(m.observe(&StressSample::default(), RELAX / 2).is_none());
        let t = m
            .observe(&StressSample::default(), RELAX / 2)
            .expect("relax elapsed");
        assert_eq!((t.from, t.to), ("Relax", "BuildUp"));
        assert!(m.intensity < PEAK_THRESHOLD, "decayed while idle");
    }

    #[test]
    fn test_allows_relax_refuses_mutating_permits_read_only() {
        let mut m = PacingManager::default();
        assert!(m.allows(false));
        m.set_phase(PacingPhase::Relax {
            until: m.clock + RELAX,
        });
        assert!(!m.allows(false));
        assert!(m.allows(true));
    }

    #[test]
    fn test_observe_engaged_never_decays_intensity() {
        let mut m = PacingManager::default();
        m.observe(&failing(1, 1), TICK);
        let before = m.intensity;
        m.observe(&ok(1), Duration::from_secs(60));
        assert_eq!(m.intensity, before);
        m.observe(&StressSample::default(), Duration::from_secs(60));
        assert_eq!(m.intensity, 0.0);
    }

    #[test]
    fn test_disrupt_returns_to_build_up_on_next_observe() {
        let mut m = PacingManager::default();
        m.disrupt("cascade".into());
        assert_eq!(m.disruption_count, 1);
        let t = m.observe(&ok(1), TICK).expect("recovers");
        assert_eq!((t.from, t.to), ("Disruption", "BuildUp"));
    }
}
