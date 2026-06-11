//! PacingManager — L4D Director-inspired phase transitions + governor GCRA
//! rate limiting.

use std::time::{Duration, Instant};

use crate::momentum::MomentumState;
use crate::tick_processor::FormationTickResult;

use super::rate_limiter::GovernorRateLimiter;
use super::types::{PacingPhase, PacingTransition};

/// Manages pacing for a formation.
pub struct PacingManager {
    pub current_phase: PacingPhase,
    pub cumulative_intensity: f32,
    pub time_since_last_recovery: Duration,
    pub disruption_count: u32,

    pub peak_duration_max: Duration,
    pub recovery_duration_min: Duration,
    pub intensity_ceiling: f32,

    rate_limiter: GovernorRateLimiter,
}

impl Default for PacingManager {
    fn default() -> Self {
        let phase = PacingPhase::Preparation {
            started: Instant::now(),
        };
        let rate_limiter = GovernorRateLimiter::for_phase(&phase);
        Self {
            current_phase: phase,
            cumulative_intensity: 0.0,
            time_since_last_recovery: Duration::ZERO,
            disruption_count: 0,
            peak_duration_max: Duration::from_secs(120),
            recovery_duration_min: Duration::from_secs(30),
            intensity_ceiling: 0.9,
            rate_limiter,
        }
    }
}

impl PacingManager {
    /// Evaluate whether a phase transition should occur.
    pub fn evaluate_transition(
        &mut self,
        tick_result: &FormationTickResult,
        momentum: &MomentumState,
        tick_elapsed: Duration,
    ) -> Option<PacingTransition> {
        self.time_since_last_recovery += tick_elapsed;

        match &self.current_phase {
            PacingPhase::Preparation { started } => {
                let has_action = tick_result.reports.iter().any(|r| r.action_taken.is_some());
                if has_action && tick_result.all_succeeded {
                    let from = self.phase_name();
                    let new_intensity = 0.3;
                    self.cumulative_intensity = new_intensity;
                    self.set_phase(PacingPhase::Active {
                        intensity: new_intensity,
                        started: *started,
                    });
                    return Some(PacingTransition { from, to: "Active" });
                }
                if !tick_result.interferences.is_empty() {
                    let from = self.phase_name();
                    self.set_phase(PacingPhase::Disruption {
                        event: "interference_during_preparation".to_owned(),
                    });
                    return Some(PacingTransition {
                        from,
                        to: "Disruption",
                    });
                }
                None
            }
            PacingPhase::Active { intensity, started } => {
                let started = *started;
                let intensity = *intensity;
                if self.cumulative_intensity > 0.7
                    && momentum.tier >= crate::momentum::MomentumTier::Hot
                {
                    let from = self.phase_name();
                    self.set_phase(PacingPhase::Peak {
                        intensity: self.cumulative_intensity,
                        fuel_rate: 2.0,
                        started,
                    });
                    return Some(PacingTransition { from, to: "Peak" });
                }
                if !tick_result.interferences.is_empty() {
                    let from = self.phase_name();
                    self.set_phase(PacingPhase::Disruption {
                        event: "interference_during_active".to_owned(),
                    });
                    return Some(PacingTransition {
                        from,
                        to: "Disruption",
                    });
                }
                if tick_result.all_succeeded {
                    let new_intensity = (intensity + 0.05).min(self.intensity_ceiling);
                    self.cumulative_intensity = new_intensity;
                    self.current_phase = PacingPhase::Active {
                        intensity: new_intensity,
                        started,
                    };
                }
                None
            }
            PacingPhase::Peak { started, .. } => {
                let started = *started;
                if started.elapsed() > self.peak_duration_max {
                    let from = self.phase_name();
                    self.set_phase(PacingPhase::Recovery {
                        remaining: self.recovery_duration_min,
                    });
                    return Some(PacingTransition {
                        from,
                        to: "Recovery",
                    });
                }
                if !tick_result.all_succeeded {
                    let from = self.phase_name();
                    self.set_phase(PacingPhase::Recovery {
                        remaining: self.recovery_duration_min,
                    });
                    return Some(PacingTransition {
                        from,
                        to: "Recovery",
                    });
                }
                None
            }
            PacingPhase::Recovery { remaining } => {
                let remaining = *remaining;
                if remaining <= tick_elapsed {
                    let from = self.phase_name();
                    self.cumulative_intensity = 0.0;
                    self.time_since_last_recovery = Duration::ZERO;
                    self.set_phase(PacingPhase::Preparation {
                        started: Instant::now(),
                    });
                    return Some(PacingTransition {
                        from,
                        to: "Preparation",
                    });
                }
                self.current_phase = PacingPhase::Recovery {
                    remaining: remaining.saturating_sub(tick_elapsed),
                };
                None
            }
            PacingPhase::Disruption { .. } => {
                let from = self.phase_name();
                self.cumulative_intensity = 0.0;
                self.set_phase(PacingPhase::Preparation {
                    started: Instant::now(),
                });
                Some(PacingTransition {
                    from,
                    to: "Preparation",
                })
            }
        }
    }

    /// Check if the next action is allowed under the current phase's GCRA
    /// rate limit. Delegates to `governor::RateLimiter::check()`.
    pub fn allow_action(&self) -> bool {
        self.rate_limiter.check()
    }

    /// Get the tick interval modifier for the current phase.
    pub fn tick_interval_modifier(&self) -> f32 {
        match &self.current_phase {
            PacingPhase::Peak { .. } => 0.5,
            PacingPhase::Recovery { .. } => 2.0,
            PacingPhase::Disruption { .. } => 1.5,
            _ => 1.0,
        }
    }

    pub fn phase_name(&self) -> &'static str {
        match &self.current_phase {
            PacingPhase::Preparation { .. } => "Preparation",
            PacingPhase::Active { .. } => "Active",
            PacingPhase::Peak { .. } => "Peak",
            PacingPhase::Recovery { .. } => "Recovery",
            PacingPhase::Disruption { .. } => "Disruption",
        }
    }

    pub fn actions_per_minute(&self) -> u32 {
        match &self.current_phase {
            PacingPhase::Preparation { .. } => 2,
            PacingPhase::Active { .. } => 10,
            PacingPhase::Peak { .. } => 30,
            PacingPhase::Recovery { .. } => 1,
            PacingPhase::Disruption { .. } => 0,
        }
    }

    pub fn disrupt(&mut self, event: String) {
        self.disruption_count += 1;
        self.set_phase(PacingPhase::Disruption { event });
    }

    /// Set a new phase and atomically swap the rate-limiter quota via
    /// `ArcSwap`. All phase transitions go through here. Concurrent
    /// readers (per-member runner tasks) see the new quota on their next
    /// `allow_action()` call without locking — the swap is lock-free.
    fn set_phase(&mut self, phase: PacingPhase) {
        self.rate_limiter.rebuild(&phase);
        self.current_phase = phase;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::cadence::{ActionDescriptor, AgentId, TickReport};

    fn tick_with_action() -> FormationTickResult {
        FormationTickResult {
            reports: vec![TickReport {
                agent_id: AgentId::new(),
                tick_sequence: crate::tick::TickId(1),
                action_taken: Some(ActionDescriptor {
                    kind: "send_message".to_owned(),
                    target: None,
                    payload_hash: 0,
                }),
                latency: Duration::from_millis(5),
                intent_alignment: 0.9,
                interference_with: vec![],
            }],
            interferences: vec![],
            all_succeeded: true,
        }
    }

    fn empty_tick() -> FormationTickResult {
        FormationTickResult {
            reports: vec![],
            interferences: vec![],
            all_succeeded: false,
        }
    }

    #[test]
    fn default_starts_in_preparation() {
        let pm = PacingManager::default();
        assert!(matches!(pm.current_phase, PacingPhase::Preparation { .. }));
        assert_eq!(pm.phase_name(), "Preparation");
    }

    #[test]
    fn preparation_to_active_on_success() {
        let mut pm = PacingManager::default();
        let t = pm.evaluate_transition(
            &tick_with_action(),
            &MomentumState::default(),
            Duration::from_millis(100),
        );
        assert_eq!(t.as_ref().map(|t| t.to), Some("Active"));
    }

    #[test]
    fn disruption_resets_to_preparation() {
        let mut pm = PacingManager::default();
        pm.disrupt("test".to_owned());
        let t = pm.evaluate_transition(
            &empty_tick(),
            &MomentumState::default(),
            Duration::from_millis(100),
        );
        assert_eq!(t.as_ref().map(|t| t.to), Some("Preparation"));
    }

    #[test]
    fn recovery_countdown_to_preparation() {
        let mut pm = PacingManager::default();
        pm.set_phase(PacingPhase::Recovery {
            remaining: Duration::from_secs(1),
        });
        assert!(
            pm.evaluate_transition(
                &empty_tick(),
                &MomentumState::default(),
                Duration::from_millis(500)
            )
            .is_none()
        );
        let t = pm.evaluate_transition(
            &empty_tick(),
            &MomentumState::default(),
            Duration::from_millis(600),
        );
        assert_eq!(t.as_ref().map(|t| t.to), Some("Preparation"));
    }

    #[test]
    fn rate_limiter_respects_phase() {
        let mut pm = PacingManager::default(); // Preparation: 2/min
        assert!(pm.allow_action());
        assert!(pm.allow_action());
        assert!(!pm.allow_action(), "3rd should be blocked at 2/min");

        // Transition to Active (10/min) — limiter rebuilt
        pm.evaluate_transition(
            &tick_with_action(),
            &MomentumState::default(),
            Duration::from_millis(100),
        );
        assert!(
            pm.allow_action(),
            "after transition to Active, budget resets"
        );
    }

    #[test]
    fn disruption_blocks_all_actions() {
        let mut pm = PacingManager::default();
        pm.disrupt("boom".to_owned());
        assert!(!pm.allow_action());
    }

    #[test]
    fn tick_interval_modifiers() {
        let mut pm = PacingManager::default();
        assert!((pm.tick_interval_modifier() - 1.0).abs() < f32::EPSILON);

        pm.set_phase(PacingPhase::Peak {
            intensity: 0.9,
            fuel_rate: 2.0,
            started: Instant::now(),
        });
        assert!((pm.tick_interval_modifier() - 0.5).abs() < f32::EPSILON);

        pm.set_phase(PacingPhase::Recovery {
            remaining: Duration::from_secs(30),
        });
        assert!((pm.tick_interval_modifier() - 2.0).abs() < f32::EPSILON);
    }
}
