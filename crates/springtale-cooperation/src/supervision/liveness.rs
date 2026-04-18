//! Liveness probe — Kubernetes-inspired agent health detection.
//!
//! Per Kubernetes: liveness probes determine when to restart a container.
//! If a probe fails `failureThreshold` times, kubelet restarts.
//! Per foca::member::State: Alive / Suspect / Down with incarnation counter.
//! Per FAILURE.md: silent failure = agent produces no heartbeat.
//!
//! Each agent reports via `TickReport` every tick. If an agent misses
//! `suspect_after` ticks, it becomes Suspect. If it misses `down_after`
//! ticks, it's declared Down and the supervisor takes action.

use serde::{Deserialize, Serialize};

/// Agent liveness state — aligned with foca::member::State.
///
/// Per Kubernetes probe model:
/// - Alive = probe succeeding, agent is operational
/// - Suspect = probe failing, not yet at threshold (K8s: container still runs)
/// - Down = threshold exceeded, restart needed (K8s: container killed)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Liveness {
    /// Agent reported on a recent tick. All good.
    #[default]
    Alive,
    /// Agent hasn't reported for `missed_ticks` ticks. Under observation.
    /// Per foca: suspicion phase before declaration of death.
    Suspect { missed_ticks: u32 },
    /// Agent hasn't reported past the down threshold. Needs recovery.
    /// Per foca: declared dead, incarnation counter bumped on rejoin.
    Down { since_tick: u64 },
}

/// Liveness probe thresholds — configurable per formation.
#[derive(Debug, Clone, Copy)]
pub struct LivenessThresholds {
    /// Ticks without a report before Suspect. Default: 5.
    pub suspect_after: u32,
    /// Ticks without a report before Down. Default: 15.
    pub down_after: u32,
}

impl Default for LivenessThresholds {
    fn default() -> Self {
        Self {
            suspect_after: 5,
            down_after: 15,
        }
    }
}

impl Liveness {
    /// Run the liveness probe for one agent.
    ///
    /// Per Kubernetes: `failureThreshold × periodSeconds` before action.
    /// Here: `suspect_after` / `down_after` measured in ticks (not seconds)
    /// because our cadence bus operates in tick-space.
    pub fn check(
        last_report_tick: u64,
        current_tick: u64,
        thresholds: &LivenessThresholds,
    ) -> Self {
        let gap = current_tick.saturating_sub(last_report_tick) as u32;
        if gap >= thresholds.down_after {
            Liveness::Down {
                since_tick: last_report_tick,
            }
        } else if gap >= thresholds.suspect_after {
            Liveness::Suspect { missed_ticks: gap }
        } else {
            Liveness::Alive
        }
    }

    pub fn is_alive(&self) -> bool {
        matches!(self, Self::Alive)
    }

    pub fn is_suspect(&self) -> bool {
        matches!(self, Self::Suspect { .. })
    }

    pub fn is_down(&self) -> bool {
        matches!(self, Self::Down { .. })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn recent_report_is_alive() {
        let thresholds = LivenessThresholds::default();
        let liveness = Liveness::check(98, 100, &thresholds);
        assert!(liveness.is_alive());
    }

    #[test]
    fn missed_ticks_become_suspect() {
        let thresholds = LivenessThresholds::default();
        let liveness = Liveness::check(90, 100, &thresholds);
        assert!(liveness.is_suspect());
        if let Liveness::Suspect { missed_ticks } = liveness {
            assert_eq!(missed_ticks, 10);
        }
    }

    #[test]
    fn many_missed_ticks_become_down() {
        let thresholds = LivenessThresholds::default();
        let liveness = Liveness::check(80, 100, &thresholds);
        assert!(liveness.is_down());
        if let Liveness::Down { since_tick } = liveness {
            assert_eq!(since_tick, 80);
        }
    }

    #[test]
    fn exact_suspect_threshold() {
        let thresholds = LivenessThresholds {
            suspect_after: 5,
            down_after: 15,
        };
        assert!(Liveness::check(95, 100, &thresholds).is_suspect());
        assert!(Liveness::check(96, 100, &thresholds).is_alive());
    }

    #[test]
    fn exact_down_threshold() {
        let thresholds = LivenessThresholds {
            suspect_after: 5,
            down_after: 15,
        };
        assert!(Liveness::check(85, 100, &thresholds).is_down());
        assert!(Liveness::check(86, 100, &thresholds).is_suspect());
    }

    #[test]
    fn zero_gap_is_alive() {
        let thresholds = LivenessThresholds::default();
        assert!(Liveness::check(100, 100, &thresholds).is_alive());
    }
}
