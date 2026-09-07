//! PacingManager — intensity is stress; at peak, back off; frequency
//! changes, amplitude never does (Booth, GDC 2009, slides 79–92).
//!
//! Every number the loop uses lives in [`PacingConfig`], per formation
//! (plan 1.5).

use std::time::{Duration, Instant};

use super::config::PacingConfig;
use super::types::{PacingPhase, PacingTransition};

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
    /// This formation's Director numbers (plan 1.5).
    pub config: PacingConfig,
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
            config: PacingConfig::default(),
            intensity: 0.0,
            disruption_count: 0,
            clock: now,
        }
    }
}

impl PacingManager {
    /// A manager on one formation's own numbers.
    pub fn with_config(config: PacingConfig) -> Self {
        Self {
            config,
            ..Self::default()
        }
    }

    /// Fold one tick's stress into intensity and advance the phase
    /// machine. `elapsed` is wall-clock time since the previous
    /// observed tick.
    pub fn observe(&mut self, s: &StressSample, elapsed: Duration) -> Option<PacingTransition> {
        let c = &self.config;
        let per_member = s.members.max(1) as f32;
        let harm = (c.w_failure * s.failures as f32
            + c.w_interference * s.interferences as f32
            + c.w_throttle * s.throttles as f32
            + c.w_denial * s.denials as f32)
            / per_member;
        let decay = c.decay_per_sec;
        let peak = c.peak_threshold;
        let sustain = c.sustain();
        let relax = c.relax();
        self.intensity = (self.intensity + harm).min(1.0);
        if !s.engaged {
            self.intensity = (self.intensity - decay * elapsed.as_secs_f32()).max(0.0);
        }
        self.clock += elapsed;
        let now = self.clock;
        let next = match &self.current_phase {
            PacingPhase::BuildUp { .. } if self.intensity >= peak => {
                Some(PacingPhase::SustainPeak { peaked_at: now })
            }
            PacingPhase::SustainPeak { peaked_at }
                if now.duration_since(*peaked_at) >= sustain =>
            {
                Some(PacingPhase::PeakFade { since: now })
            }
            // Booth: "Peak Fade won't allow the Relax period to start
            // until a natural break in the action occurs."
            PacingPhase::PeakFade { .. } if !s.engaged || self.intensity < peak => {
                Some(PacingPhase::Relax { until: now + relax })
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
        let sustain = m.config.sustain();
        let t = m.observe(&ok(2), sustain).expect("sustain elapsed");
        assert_eq!((t.from, t.to), ("SustainPeak", "PeakFade"));
        assert_eq!(m.tick_divider(), 2);
        // Peak fade waits for a natural break: not engaged.
        let t = m.observe(&StressSample::default(), TICK).expect("break");
        assert_eq!((t.from, t.to), ("PeakFade", "Relax"));
        assert_eq!(m.tick_divider(), 4);
        // Relax returns to BuildUp once the relax period elapses.
        let relax = m.config.relax();
        assert!(m.observe(&StressSample::default(), relax / 2).is_none());
        let t = m
            .observe(&StressSample::default(), relax / 2)
            .expect("relax elapsed");
        assert_eq!((t.from, t.to), ("Relax", "BuildUp"));
        assert!(m.intensity < m.config.peak_threshold, "decayed while idle");
    }

    #[test]
    fn test_allows_relax_refuses_mutating_permits_read_only() {
        let mut m = PacingManager::default();
        assert!(m.allows(false));
        let until = m.clock + m.config.relax();
        m.set_phase(PacingPhase::Relax { until });
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod config_tests {
    use super::*;

    /// Plan 1.5: every Director number is configuration. A formation
    /// tuned to peak early and hold longer does exactly that; the
    /// defaults are unchanged for everyone else.
    #[test]
    fn test_observe_uses_the_formations_own_numbers() {
        let mut m = PacingManager::with_config(PacingConfig {
            peak_threshold: 0.1,
            sustain_secs: 60,
            w_failure: 1.0,
            ..PacingConfig::default()
        });
        let stressed = StressSample {
            failures: 1,
            members: 1,
            engaged: true,
            ..StressSample::default()
        };
        let t = m
            .observe(&stressed, Duration::from_millis(33))
            .expect("one failure at weight 1.0 clears a 0.1 peak");
        assert_eq!(t.to, "SustainPeak");

        // The default manager needs far more than one failure to peak.
        let mut d = PacingManager::default();
        assert!(d.observe(&stressed, Duration::from_millis(33)).is_none());
        assert!(d.intensity < d.config.peak_threshold);

        // A 60-second sustain does not fade after the default 4.
        assert!(m.observe(&stressed, Duration::from_secs(5)).is_none());
    }
}
