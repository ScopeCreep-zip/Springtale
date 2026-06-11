//! FormationSupervisor — the lifecycle manager for formation members.
//!
//! Combines Erlang OTP restart intensity, Kubernetes liveness probes,
//! FAILURE.md classification, and AutoGen tiered retry into one decision
//! function called per member per tick.
//!
//! The supervisor does NOT execute actions itself — it returns a
//! `SupervisionAction` that the event loop dispatches. This keeps the
//! supervisor pure and testable without needing a Formation reference.

use crate::cadence::AgentId;
use crate::rally::RallyTokens;

use super::failure::{self, FailureCategory};
use super::liveness::Liveness;
use super::restart::RestartPolicy;

/// What the supervisor tells the event loop to do about a member.
#[derive(Debug, Clone)]
pub enum SupervisionAction {
    /// Transform the agent's role (§14) — alive but degraded.
    /// Per OpenAI Swarm: handoff to a different agent persona.
    TransformRole { agent: AgentId },
    /// Retry with rally token (§15) — consume token, reset member.
    /// Per AutoGen: attempt with error feedback.
    RetryWithRally { agent: AgentId },
    /// Trigger CBBA replan (L5) — formation-wide task reallocation.
    /// Per FAILURE.md: circuit breaker activated.
    TriggerReplan,
    /// Mark agent Down, broadcast PeerMsg::AgentDown.
    /// Per Kubernetes: liveness probe failed → restart container.
    MarkDown {
        agent: AgentId,
        since_tick: crate::tick::TickId,
    },
    /// Escalate to L6 intervention — restart budget exhausted.
    /// Per Erlang: supervisor exceeded MaxR/MaxT → terminate.
    Escalate { reason: String },
}

/// Per Erlang OTP: supervisor tracks restart history and enforces intensity.
/// Per FAILURE.md: graduated response from degrade → retry → replan → escalate.
/// Per AutoGen: checkpointing — track last successful tick per member.
pub struct FormationSupervisor {
    pub policy: RestartPolicy,
    restart_history: Vec<crate::tick::TickId>,
}

impl FormationSupervisor {
    pub fn new(policy: RestartPolicy) -> Self {
        Self {
            policy,
            restart_history: Vec::new(),
        }
    }

    /// Called per member per tick. Returns an action if intervention needed.
    ///
    /// The event loop fills in the actual `agent` field on the returned
    /// action — the supervisor doesn't know member identity, only signals.
    pub fn check_member(
        &self,
        agent: AgentId,
        liveness: Liveness,
        consecutive_failures: usize,
        cascade_signals: u32,
        rally: &RallyTokens,
    ) -> Option<SupervisionAction> {
        let category = failure::classify(liveness, consecutive_failures, cascade_signals)?;

        match category {
            FailureCategory::GracefulDegradation => {
                Some(SupervisionAction::TransformRole { agent })
            }
            FailureCategory::PartialFailure => {
                if rally.can_rally() {
                    Some(SupervisionAction::RetryWithRally { agent })
                } else {
                    Some(SupervisionAction::Escalate {
                        reason: "rally tokens exhausted during partial failure".to_owned(),
                    })
                }
            }
            FailureCategory::CascadingFailure => Some(SupervisionAction::TriggerReplan),
            FailureCategory::SilentFailure => {
                if let Liveness::Down { since_tick } = liveness {
                    Some(SupervisionAction::MarkDown { agent, since_tick })
                } else {
                    None
                }
            }
        }
    }

    /// Per Erlang OTP: track restart, check intensity (MaxR in MaxT).
    /// Returns `true` if restart is within budget, `false` if intensity
    /// exceeded (caller should escalate).
    pub fn record_restart(&mut self, current_tick: crate::tick::TickId) -> bool {
        self.restart_history.push(current_tick);

        // Prune restarts outside the window
        let cutoff = crate::tick::TickId(current_tick.0.saturating_sub(self.policy.within_ticks));
        self.restart_history.retain(|t| *t >= cutoff);

        // Check intensity
        self.restart_history.len() <= self.policy.max_restarts as usize
    }

    /// How many restarts have occurred in the current window.
    pub fn restarts_in_window(&self) -> usize {
        self.restart_history.len()
    }

    /// Whether the restart budget is exhausted.
    pub fn intensity_exceeded(&self) -> bool {
        self.restart_history.len() > self.policy.max_restarts as usize
    }
}

impl Default for FormationSupervisor {
    fn default() -> Self {
        Self::new(RestartPolicy::default())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn healthy_member_no_action() {
        let sup = FormationSupervisor::default();
        let rally = RallyTokens::new(3);
        let action = sup.check_member(AgentId::new(), Liveness::Alive, 0, 0, &rally);
        assert!(action.is_none());
    }

    #[test]
    fn down_member_gets_mark_down() {
        let sup = FormationSupervisor::default();
        let rally = RallyTokens::new(3);
        let action = sup.check_member(
            AgentId::new(),
            Liveness::Down {
                since_tick: crate::tick::TickId(80),
            },
            0,
            0,
            &rally,
        );
        assert!(matches!(
            action,
            Some(SupervisionAction::MarkDown {
                since_tick: crate::tick::TickId(80),
                ..
            })
        ));
    }

    #[test]
    fn cascade_triggers_replan() {
        let sup = FormationSupervisor::default();
        let rally = RallyTokens::new(3);
        let action = sup.check_member(
            AgentId::new(),
            Liveness::Suspect { missed_ticks: 8 },
            0,
            4,
            &rally,
        );
        assert!(matches!(action, Some(SupervisionAction::TriggerReplan)));
    }

    #[test]
    fn five_failures_transforms_role() {
        let sup = FormationSupervisor::default();
        let rally = RallyTokens::new(3);
        let action = sup.check_member(AgentId::new(), Liveness::Alive, 5, 0, &rally);
        assert!(matches!(
            action,
            Some(SupervisionAction::TransformRole { .. })
        ));
    }

    #[test]
    fn partial_failure_with_rally_retries() {
        let sup = FormationSupervisor::default();
        let rally = RallyTokens::new(3);
        let action = sup.check_member(AgentId::new(), Liveness::Alive, 3, 0, &rally);
        assert!(matches!(
            action,
            Some(SupervisionAction::RetryWithRally { .. })
        ));
    }

    #[test]
    fn partial_failure_no_rally_escalates() {
        let sup = FormationSupervisor::default();
        let rally = RallyTokens::new(3);
        rally.consume().unwrap();
        rally.consume().unwrap();
        rally.consume().unwrap(); // exhausted

        let action = sup.check_member(AgentId::new(), Liveness::Alive, 3, 0, &rally);
        assert!(matches!(action, Some(SupervisionAction::Escalate { .. })));
    }

    #[test]
    fn restart_within_budget_returns_true() {
        let mut sup = FormationSupervisor::new(RestartPolicy {
            max_restarts: 3,
            within_ticks: 100,
            strategy: super::super::restart::RestartStrategy::OneForOne,
        });
        assert!(sup.record_restart(crate::tick::TickId(10)));
        assert!(sup.record_restart(crate::tick::TickId(20)));
        assert!(sup.record_restart(crate::tick::TickId(30)));
        assert_eq!(sup.restarts_in_window(), 3);
    }

    #[test]
    fn restart_exceeding_budget_returns_false() {
        let mut sup = FormationSupervisor::new(RestartPolicy {
            max_restarts: 2,
            within_ticks: 100,
            strategy: super::super::restart::RestartStrategy::OneForOne,
        });
        assert!(sup.record_restart(crate::tick::TickId(10)));
        assert!(sup.record_restart(crate::tick::TickId(20)));
        assert!(!sup.record_restart(crate::tick::TickId(30))); // 3rd restart exceeds budget of 2
    }

    #[test]
    fn old_restarts_pruned_from_window() {
        let mut sup = FormationSupervisor::new(RestartPolicy {
            max_restarts: 2,
            within_ticks: 50,
            strategy: super::super::restart::RestartStrategy::OneForOne,
        });
        assert!(sup.record_restart(crate::tick::TickId(10)));
        assert!(sup.record_restart(crate::tick::TickId(20)));
        // Tick 70: restart at 10 is outside window (70-50=20), so only restart at 20 counts
        assert!(sup.record_restart(crate::tick::TickId(70)));
        assert_eq!(sup.restarts_in_window(), 2); // 20 and 70
    }
}
