//! Synchronized commit — coordinated execution barriers (§12).
//!
//! Game sources (COOPERATION.md §12): Splinter Cell Conviction dual breach,
//! Army of Two co-op snipe. "Both players stack on opposite sides of a
//! door. One initiates a countdown. Both must execute simultaneously.
//! Failure of either after commit exposes both."
//!
//! ## When it's available
//!
//! Synchronized commit is a `Hot+` capability (§7 capability table).
//! Formations below Hot can't use it — the primitive is gated because
//! the all-or-nothing execution only pays off when coherence is high
//! enough that `Prepare → Ready` transitions reliably succeed. Cold or
//! Warming formations fall back to sequential or best-effort execution.
//!
//! ## Lifecycle
//!
//! ```text
//!   Prepare ── all signal_ready ──▶ Ready ── tick() ──▶ Execute
//!      │  prepare deadline expires       │  execute deadline expires
//!      │  any record_prepare_failure     │  unreported → Failed
//!      ▼                                 ▼
//!    Aborted                          Collect
//! ```
//!
//! The tick-driver (`Formation::tick_commits` / `tick_steps::tick_commits`)
//! calls `CommitBarrier::tick(now)` once per cooperation tick. The
//! barrier transitions phases autonomously based on participant
//! readiness, per-phase deadlines, and fail-fast policy. Each transition
//! returns a `CommitTransition` value so the caller can emit a
//! `CooperationEvent::CommitPhaseChanged` envelope (Phase H5).
//!
//! ## Fail-fast prepare
//!
//! Per Splinter Cell's "if either of you flinches, both of you die"
//! semantic, a single `record_prepare_failure` during Prepare flips the
//! entire barrier to `Aborted` immediately — peers don't have to wait
//! for the deadline. Mid-execute failures are *not* fail-fast: every
//! participant gets recorded before transitioning to Collect, so the
//! formation can audit which agent failed (parity with Army of Two's
//! "down" state — failed players are visible to the team rather than
//! disappearing).
//!
//! ## Why oneshot channels are not used
//!
//! An earlier draft used `Barrier` because "N parties meet" maps
//! naturally. Two problems killed it:
//!
//! 1. **Cancel safety.** `Barrier::wait` is not cancel-safe — a dropped
//!    future inside a `tokio::select!` arm leaves the barrier in a
//!    poisoned state that other callers then hang on forever.
//! 2. **Opaque state.** `Barrier` exposes neither per-participant state
//!    nor a deadline hook, so the formation couldn't tell *which* agent
//!    was late — only that the whole barrier failed.
//!
//! Tick-driven explicit state machines sidestep both: each transition is
//! observable, terminal states are inspectable, and no future is parked.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::action::SubTaskResult;
use crate::cadence::AgentId;
use crate::error::CommitError;

/// Default deadline budget for the Prepare phase if the caller didn't
/// override it via `CommitBarrier::with_phase_deadlines`.
pub const DEFAULT_PREPARE_DEADLINE: Duration = Duration::from_secs(10);
/// Default deadline budget for the Execute phase. Execute pays an
/// additional grace window because connectors do real I/O here.
pub const DEFAULT_EXECUTE_DEADLINE: Duration = Duration::from_secs(30);

/// Phases of a synchronized commit operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitPhase {
    /// Agents are preparing their part of the operation.
    Prepare,
    /// All agents have signaled readiness.
    Ready,
    /// Countdown to synchronized execution (Splinter Cell breach timer).
    /// Entered from Ready when the barrier was built `with_countdown`;
    /// `tick()` decrements `remaining` and advances to Execute at zero.
    Countdown { remaining: Duration },
    /// Executing simultaneously.
    Execute,
    /// Collecting results from all participants.
    Collect,
    /// Barrier aborted before commit (prepare deadline missed, or any
    /// participant called `record_prepare_failure`). Carries the reason
    /// so observers can surface it in audit logs / UI toasts.
    Aborted { reason: String },
}

impl CommitPhase {
    /// Short identifier suitable for `CooperationEvent::CommitPhaseChanged`
    /// payloads. Stable strings so the frontend can branch on them.
    pub fn as_event_str(&self) -> &'static str {
        match self {
            CommitPhase::Prepare => "prepare",
            CommitPhase::Ready => "ready",
            CommitPhase::Countdown { .. } => "countdown",
            CommitPhase::Execute => "execute",
            CommitPhase::Collect => "collect",
            CommitPhase::Aborted { .. } => "aborted",
        }
    }

    /// `true` if the phase is terminal — `Collect` or `Aborted`. Used by
    /// `expire_commits` to decide whether to drop a barrier from
    /// `formation.active_commits`.
    pub fn is_terminal(&self) -> bool {
        matches!(self, CommitPhase::Collect | CommitPhase::Aborted { .. })
    }
}

/// State of a single participant in a commit barrier.
#[derive(Debug, Clone)]
pub enum ParticipantState {
    /// Still preparing.
    Preparing,
    /// Signaled readiness.
    Ready,
    /// Currently executing.
    Executing,
    /// Completed with result.
    Completed(SubTaskResult),
    /// Failed during execution.
    Failed(String),
}

/// One phase change observed during a `tick()` call. The driver emits a
/// `CooperationEvent::CommitPhaseChanged` envelope per transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitTransition {
    pub from: &'static str,
    pub to: &'static str,
}

/// A synchronized commit barrier — two-phase commit for formation actions.
///
/// Lifecycle: Prepare → Ready → Execute → Collect (or Aborted).
///
/// Per spec §12: "Cooperation is in planning; execution is deterministic."
/// All participants must signal ready before anyone executes. If the
/// prepare deadline expires before all are ready, the barrier aborts. If
/// any participant flags a prepare-failure, the barrier aborts immediately.
pub struct CommitBarrier {
    /// Unique barrier identifier.
    pub id: uuid::Uuid,
    /// Current phase.
    pub phase: CommitPhase,
    /// Per-participant state.
    pub participants: HashMap<AgentId, ParticipantState>,
    /// When the *prepare* phase expires. Once the barrier advances past
    /// Prepare, this becomes the execute-phase deadline.
    pub deadline: Instant,
    /// Per-phase deadline budget for execute (set when the barrier
    /// transitions to Execute via `tick()`). Held separately so the
    /// prepare timeout is independent of the execute timeout.
    execute_deadline: Duration,
    /// Splinter-Cell breach countdown: time the barrier holds in
    /// `Countdown` between Ready and Execute. `Duration::ZERO` (the
    /// default) skips the phase entirely — Ready advances straight to
    /// Execute, preserving pre-countdown behavior.
    countdown: Duration,
    /// Who initiated the barrier.
    pub initiated_by: AgentId,
}

impl CommitBarrier {
    /// Create a new commit barrier in Prepare phase with default per-phase
    /// deadlines. `deadline` becomes the *prepare* deadline; the execute
    /// phase uses `DEFAULT_EXECUTE_DEADLINE` once advanced.
    pub fn new(participants: &[AgentId], deadline: Duration, initiated_by: AgentId) -> Self {
        Self::with_phase_deadlines(
            participants,
            deadline,
            DEFAULT_EXECUTE_DEADLINE,
            initiated_by,
        )
    }

    /// Like `new`, but lets callers override the execute deadline budget
    /// independently of the prepare deadline. Used by high-priority
    /// barriers that need a tighter execute window after Ready.
    pub fn with_phase_deadlines(
        participants: &[AgentId],
        prepare_deadline: Duration,
        execute_deadline: Duration,
        initiated_by: AgentId,
    ) -> Self {
        let participant_map = participants
            .iter()
            .map(|id| (*id, ParticipantState::Preparing))
            .collect();

        Self {
            id: uuid::Uuid::new_v4(),
            phase: CommitPhase::Prepare,
            participants: participant_map,
            deadline: Instant::now() + prepare_deadline,
            execute_deadline,
            countdown: Duration::ZERO,
            initiated_by,
        }
    }

    /// Set a Splinter-Cell-style breach countdown (§12.1): once every
    /// participant is Ready, the barrier holds in `Countdown` for `d`
    /// before Execute. All peers observe the `ready → countdown →
    /// execute` transitions, so UI / audit surfaces can render the
    /// synchronized "3…2…1" window.
    pub fn with_countdown(mut self, d: Duration) -> Self {
        self.countdown = d;
        self
    }

    /// Signal that an agent is ready.
    ///
    /// Returns Ok(()) if accepted, error if agent not in barrier or
    /// barrier is past the Prepare phase.
    pub fn signal_ready(&mut self, agent_id: AgentId) -> Result<(), CommitError> {
        if self.phase != CommitPhase::Prepare {
            return Err(CommitError::BarrierFailed(
                "barrier past prepare phase".to_owned(),
            ));
        }

        let state = self
            .participants
            .get_mut(&agent_id)
            .ok_or(CommitError::AgentNotInBarrier(agent_id))?;

        *state = ParticipantState::Ready;

        // Auto-advance to Ready when all participants are ready
        if self.all_ready() {
            self.phase = CommitPhase::Ready;
        }

        Ok(())
    }

    /// Record that a participant failed to prepare. Fail-fast: this
    /// transitions the entire barrier to `Aborted` immediately, peers
    /// don't wait for the deadline.
    ///
    /// Returns the transition if the barrier moved into Aborted; `None`
    /// if the barrier was already past Prepare (late prepare failures
    /// are treated as execute failures via `record_failure`).
    pub fn record_prepare_failure(
        &mut self,
        agent_id: AgentId,
        reason: impl Into<String>,
    ) -> Option<CommitTransition> {
        if !matches!(self.phase, CommitPhase::Prepare) {
            return None;
        }
        let reason: String = reason.into();
        if let Some(state) = self.participants.get_mut(&agent_id) {
            *state = ParticipantState::Failed(reason.clone());
        }
        let from = self.phase.as_event_str();
        self.phase = CommitPhase::Aborted {
            reason: format!("{agent_id} failed prepare: {reason}"),
        };
        Some(CommitTransition {
            from,
            to: self.phase.as_event_str(),
        })
    }

    /// Check if all participants have signaled ready.
    pub fn all_ready(&self) -> bool {
        !self.participants.is_empty()
            && self
                .participants
                .values()
                .all(|s| matches!(s, ParticipantState::Ready))
    }

    /// Advance from Ready to Execute phase.
    ///
    /// All participants are marked as Executing. Resets the deadline
    /// counter to the execute-phase budget. Most callers should rely on
    /// `tick()` to do this automatically; `execute()` is exposed for
    /// callers that need synchronous control (e.g. tests).
    pub fn execute(&mut self) -> Result<(), CommitError> {
        if self.phase != CommitPhase::Ready {
            return Err(CommitError::BarrierFailed(
                "barrier not in ready phase".to_owned(),
            ));
        }

        self.phase = CommitPhase::Execute;
        self.deadline = Instant::now() + self.execute_deadline;
        for state in self.participants.values_mut() {
            *state = ParticipantState::Executing;
        }

        Ok(())
    }

    /// Record a participant's execution result.
    pub fn collect_result(&mut self, agent_id: AgentId, result: SubTaskResult) {
        if let Some(state) = self.participants.get_mut(&agent_id) {
            *state = ParticipantState::Completed(result);
        } else {
            tracing::warn!(
                barrier = %self.id,
                agent = %agent_id.0,
                "collect_result: agent not in barrier participants"
            );
        }

        // Auto-advance to Collect when all results are in
        if self.all_collected() {
            self.phase = CommitPhase::Collect;
        }
    }

    /// Record a participant's failure.
    pub fn record_failure(&mut self, agent_id: AgentId, reason: String) {
        if let Some(state) = self.participants.get_mut(&agent_id) {
            *state = ParticipantState::Failed(reason);
        } else {
            tracing::warn!(
                barrier = %self.id,
                agent = %agent_id.0,
                "record_failure: agent not in barrier participants"
            );
        }

        // Any failure transitions to Collect once all resolved
        if self.all_resolved() {
            self.phase = CommitPhase::Collect;
        }
    }

    /// Drive the barrier forward by one cooperation tick. Returns the
    /// phase transition (if any) so the caller can emit a
    /// `CommitPhaseChanged` event. Idempotent within a phase.
    ///
    /// Transitions handled:
    /// - **Prepare → Aborted** when the prepare deadline has elapsed.
    /// - **Ready → Execute** unconditionally (the auto-advance in
    ///   `signal_ready` flipped the phase to Ready already; this step
    ///   starts the execute clock and marks participants Executing).
    /// - **Execute → Collect** when the execute deadline has elapsed —
    ///   any participant that hadn't reported is marked `Failed("execute timeout")`.
    pub fn tick(&mut self, now: Instant) -> Option<CommitTransition> {
        match self.phase.clone() {
            CommitPhase::Prepare => {
                if now >= self.deadline {
                    let from = self.phase.as_event_str();
                    self.phase = CommitPhase::Aborted {
                        reason: "prepare deadline expired".to_owned(),
                    };
                    Some(CommitTransition {
                        from,
                        to: self.phase.as_event_str(),
                    })
                } else {
                    None
                }
            }
            CommitPhase::Ready => {
                let from = self.phase.as_event_str();
                if self.countdown > Duration::ZERO {
                    // §12.1 breach countdown — hold before simultaneous
                    // execution so every peer sees the window coming. The
                    // shared `deadline` field carries the countdown expiry;
                    // `remaining` is the display value updated each tick.
                    self.deadline = now + self.countdown;
                    self.phase = CommitPhase::Countdown {
                        remaining: self.countdown,
                    };
                } else {
                    self.phase = CommitPhase::Execute;
                    self.deadline = now + self.execute_deadline;
                    for state in self.participants.values_mut() {
                        *state = ParticipantState::Executing;
                    }
                }
                Some(CommitTransition {
                    from,
                    to: self.phase.as_event_str(),
                })
            }
            CommitPhase::Countdown { .. } => {
                if now >= self.deadline {
                    // Countdown elapsed — simultaneous execution begins.
                    let from = self.phase.as_event_str();
                    self.phase = CommitPhase::Execute;
                    self.deadline = now + self.execute_deadline;
                    for state in self.participants.values_mut() {
                        *state = ParticipantState::Executing;
                    }
                    Some(CommitTransition {
                        from,
                        to: self.phase.as_event_str(),
                    })
                } else {
                    // Refresh the display value; not a phase transition.
                    self.phase = CommitPhase::Countdown {
                        remaining: self.deadline - now,
                    };
                    None
                }
            }
            CommitPhase::Execute => {
                if now >= self.deadline {
                    let from = self.phase.as_event_str();
                    let mut timed_out = Vec::new();
                    for (agent, state) in self.participants.iter_mut() {
                        if matches!(state, ParticipantState::Executing) {
                            *state =
                                ParticipantState::Failed("execute deadline expired".to_owned());
                            timed_out.push(*agent);
                        }
                    }
                    if !timed_out.is_empty() {
                        tracing::warn!(
                            barrier = %self.id,
                            timed_out = ?timed_out,
                            "commit barrier execute timeout — unreported agents marked Failed"
                        );
                    }
                    self.phase = CommitPhase::Collect;
                    Some(CommitTransition {
                        from,
                        to: self.phase.as_event_str(),
                    })
                } else {
                    None
                }
            }
            CommitPhase::Collect | CommitPhase::Aborted { .. } => None,
        }
    }

    /// Check if all participants have completed or failed.
    fn all_resolved(&self) -> bool {
        self.participants.values().all(|s| {
            matches!(
                s,
                ParticipantState::Completed(_) | ParticipantState::Failed(_)
            )
        })
    }

    /// Check if all participants completed successfully.
    fn all_collected(&self) -> bool {
        !self.participants.is_empty()
            && self
                .participants
                .values()
                .all(|s| matches!(s, ParticipantState::Completed(_)))
    }

    /// Check if the barrier's current-phase deadline has expired. Used
    /// alongside `tick()` for callers that need to peek without
    /// transitioning.
    pub fn is_expired(&self) -> bool {
        Instant::now() >= self.deadline
    }

    /// Check if the barrier has reached a terminal phase (Collect or
    /// Aborted). `expire_commits.rs` uses this to drop finished barriers.
    pub fn is_complete(&self) -> bool {
        self.phase.is_terminal()
    }

    /// Check if any participant failed.
    pub fn has_failures(&self) -> bool {
        self.participants
            .values()
            .any(|s| matches!(s, ParticipantState::Failed(_)))
    }

    /// Check whether the barrier ended in the explicit Aborted phase
    /// (distinguishable from a Collect with mixed results).
    pub fn was_aborted(&self) -> bool {
        matches!(self.phase, CommitPhase::Aborted { .. })
    }

    /// Get the count of participants.
    pub fn participant_count(&self) -> usize {
        self.participants.len()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn make_result(agent: AgentId) -> SubTaskResult {
        SubTaskResult {
            task_id: uuid::Uuid::new_v4(),
            agent_id: agent,
            success: true,
            output: serde_json::json!({"status": "ok"}),
            duration_ms: 50,
        }
    }

    #[test]
    fn test_full_lifecycle() {
        let a = AgentId::new();
        let b = AgentId::new();
        let c = AgentId::new();

        let mut barrier = CommitBarrier::new(&[a, b, c], Duration::from_secs(30), a);
        assert_eq!(barrier.phase, CommitPhase::Prepare);
        assert_eq!(barrier.participant_count(), 3);

        // All signal ready
        barrier.signal_ready(a).unwrap();
        assert_eq!(barrier.phase, CommitPhase::Prepare); // not all ready yet
        barrier.signal_ready(b).unwrap();
        barrier.signal_ready(c).unwrap();
        assert_eq!(barrier.phase, CommitPhase::Ready); // auto-advanced

        // Execute
        barrier.execute().unwrap();
        assert_eq!(barrier.phase, CommitPhase::Execute);

        // Collect results
        barrier.collect_result(a, make_result(a));
        barrier.collect_result(b, make_result(b));
        assert!(!barrier.is_complete()); // c hasn't reported
        barrier.collect_result(c, make_result(c));
        assert!(barrier.is_complete());
        assert!(!barrier.has_failures());
        assert!(!barrier.was_aborted());
    }

    #[test]
    fn test_signal_ready_unknown_agent() {
        let a = AgentId::new();
        let unknown = AgentId::new();
        let mut barrier = CommitBarrier::new(&[a], Duration::from_secs(30), a);
        let result = barrier.signal_ready(unknown);
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_before_ready() {
        let a = AgentId::new();
        let mut barrier = CommitBarrier::new(&[a], Duration::from_secs(30), a);
        let result = barrier.execute();
        assert!(result.is_err());
    }

    #[test]
    fn test_failure_during_execution() {
        let a = AgentId::new();
        let b = AgentId::new();
        let mut barrier = CommitBarrier::new(&[a, b], Duration::from_secs(30), a);

        barrier.signal_ready(a).unwrap();
        barrier.signal_ready(b).unwrap();
        barrier.execute().unwrap();

        barrier.collect_result(a, make_result(a));
        barrier.record_failure(b, "connector timeout".to_owned());

        assert!(barrier.is_complete());
        assert!(barrier.has_failures());
        assert!(!barrier.was_aborted());
    }

    #[test]
    fn test_deadline_expiry() {
        let a = AgentId::new();
        let barrier = CommitBarrier::new(&[a], Duration::from_secs(0), a);
        // Duration::from_secs(0) means instant expiry
        assert!(barrier.is_expired());
    }

    #[test]
    fn test_signal_ready_after_ready_phase() {
        let a = AgentId::new();
        let mut barrier = CommitBarrier::new(&[a], Duration::from_secs(30), a);
        barrier.signal_ready(a).unwrap();
        assert_eq!(barrier.phase, CommitPhase::Ready);

        // Can't signal ready again after phase advanced
        let late = AgentId::new();
        // late isn't in the barrier, but even if they were, phase check comes first
        let result = barrier.signal_ready(late);
        assert!(result.is_err());
    }

    #[test]
    fn tick_aborts_on_prepare_deadline() {
        let a = AgentId::new();
        let mut barrier = CommitBarrier::new(&[a], Duration::from_millis(0), a);
        let t = barrier.tick(Instant::now() + Duration::from_millis(1));
        assert_eq!(
            t,
            Some(CommitTransition {
                from: "prepare",
                to: "aborted"
            })
        );
        assert!(barrier.was_aborted());
        assert!(barrier.is_complete());
    }

    #[test]
    fn tick_advances_ready_to_execute() {
        let a = AgentId::new();
        let mut barrier = CommitBarrier::new(&[a], Duration::from_secs(30), a);
        barrier.signal_ready(a).unwrap();
        assert_eq!(barrier.phase, CommitPhase::Ready);
        let t = barrier.tick(Instant::now());
        assert_eq!(
            t,
            Some(CommitTransition {
                from: "ready",
                to: "execute"
            })
        );
        assert_eq!(barrier.phase, CommitPhase::Execute);
        // Subsequent tick within execute deadline is a no-op
        assert!(barrier.tick(Instant::now()).is_none());
    }

    #[test]
    fn tick_aborts_unreported_on_execute_deadline() {
        let a = AgentId::new();
        let b = AgentId::new();
        let mut barrier = CommitBarrier::with_phase_deadlines(
            &[a, b],
            Duration::from_secs(30),
            Duration::from_millis(0),
            a,
        );
        barrier.signal_ready(a).unwrap();
        barrier.signal_ready(b).unwrap();
        // Ready → Execute, deadline 0ms → next tick expires execute.
        let t1 = barrier.tick(Instant::now());
        assert_eq!(t1.map(|x| x.to), Some("execute"));
        let t2 = barrier.tick(Instant::now() + Duration::from_millis(1));
        assert_eq!(t2.map(|x| x.to), Some("collect"));
        assert!(barrier.has_failures());
        assert!(barrier.is_complete());
    }

    #[test]
    fn prepare_failure_aborts_fast() {
        let a = AgentId::new();
        let b = AgentId::new();
        let mut barrier = CommitBarrier::new(&[a, b], Duration::from_secs(30), a);
        let t = barrier.record_prepare_failure(b, "lost connector");
        assert!(t.is_some());
        assert_eq!(t.unwrap().to, "aborted");
        assert!(barrier.was_aborted());
        // Late signal_ready calls after abort must error.
        assert!(barrier.signal_ready(a).is_err());
    }

    #[test]
    fn prepare_failure_after_ready_returns_none() {
        let a = AgentId::new();
        let mut barrier = CommitBarrier::new(&[a], Duration::from_secs(30), a);
        barrier.signal_ready(a).unwrap();
        // Already Ready — record_prepare_failure must not abort.
        let t = barrier.record_prepare_failure(a, "too late");
        assert!(t.is_none());
        assert_eq!(barrier.phase, CommitPhase::Ready);
    }

    #[test]
    fn zero_countdown_goes_straight_to_execute() {
        let a = AgentId::new();
        let mut barrier = CommitBarrier::new(&[a], Duration::from_secs(30), a);
        barrier.signal_ready(a).unwrap();
        let t = barrier.tick(Instant::now()).unwrap();
        assert_eq!(t.from, "ready");
        assert_eq!(t.to, "execute", "ZERO countdown preserves old behavior");
    }

    #[test]
    fn countdown_holds_then_executes() {
        let a = AgentId::new();
        let mut barrier = CommitBarrier::new(&[a], Duration::from_secs(30), a)
            .with_countdown(Duration::from_millis(50));
        barrier.signal_ready(a).unwrap();

        // Ready → Countdown
        let now = Instant::now();
        let t = barrier.tick(now).unwrap();
        assert_eq!(t.to, "countdown");

        // Mid-countdown: no transition, remaining shrinks.
        let t = barrier.tick(now + Duration::from_millis(20));
        assert!(t.is_none());
        match &barrier.phase {
            CommitPhase::Countdown { remaining } => {
                assert!(*remaining <= Duration::from_millis(30));
            }
            other => panic!("expected Countdown, got {other:?}"),
        }

        // Past the countdown: → Execute, all participants Executing.
        let t = barrier.tick(now + Duration::from_millis(60)).unwrap();
        assert_eq!(t.from, "countdown");
        assert_eq!(t.to, "execute");
        assert!(
            barrier
                .participants
                .values()
                .all(|s| matches!(s, ParticipantState::Executing))
        );
    }

    #[test]
    fn phase_event_strs_are_stable() {
        assert_eq!(CommitPhase::Prepare.as_event_str(), "prepare");
        assert_eq!(CommitPhase::Ready.as_event_str(), "ready");
        assert_eq!(
            CommitPhase::Countdown {
                remaining: Duration::from_secs(1)
            }
            .as_event_str(),
            "countdown"
        );
        assert_eq!(CommitPhase::Execute.as_event_str(), "execute");
        assert_eq!(CommitPhase::Collect.as_event_str(), "collect");
        assert_eq!(
            CommitPhase::Aborted { reason: "x".into() }.as_event_str(),
            "aborted"
        );
    }
}
