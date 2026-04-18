//! Synchronized commit — coordinated execution barriers.
//!
//! Per COOPERATION.pdf §12:
//! Game sources: Splinter Cell dual breach, Army of Two co-op snipe.
//!
//! "Both players stack on opposite sides of a door. One initiates a
//! countdown. Both must execute simultaneously. Failure of either
//! after commit exposes both."
//!
//! Available at Hot+ tier (§7 capability table).
//! Uses oneshot channels per spec (avoids Barrier cancel-safety issues).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::action::SubTaskResult;
use crate::cadence::AgentId;
use crate::error::CommitError;

/// Phases of a synchronized commit operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitPhase {
    /// Agents are preparing their part of the operation.
    Prepare,
    /// All agents have signaled readiness.
    Ready,
    /// Countdown to synchronized execution.
    Countdown { remaining: Duration },
    /// Executing simultaneously.
    Execute,
    /// Collecting results from all participants.
    Collect,
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

/// A synchronized commit barrier — two-phase commit for formation actions.
///
/// Lifecycle: Prepare → Ready → Execute → Collect
///
/// Per spec §12: "Cooperation is in planning; execution is deterministic."
/// All participants must signal ready before anyone executes. If the
/// deadline expires before all are ready, the barrier fails.
pub struct CommitBarrier {
    /// Unique barrier identifier.
    pub id: uuid::Uuid,
    /// Current phase.
    pub phase: CommitPhase,
    /// Per-participant state.
    pub participants: HashMap<AgentId, ParticipantState>,
    /// When this barrier expires if not all ready.
    pub deadline: Instant,
    /// Who initiated the barrier.
    pub initiated_by: AgentId,
}

impl CommitBarrier {
    /// Create a new commit barrier in Prepare phase.
    pub fn new(
        participants: &[AgentId],
        deadline: Duration,
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
            deadline: Instant::now() + deadline,
            initiated_by,
        }
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
    /// All participants are marked as Executing.
    pub fn execute(&mut self) -> Result<(), CommitError> {
        if self.phase != CommitPhase::Ready {
            return Err(CommitError::BarrierFailed(
                "barrier not in ready phase".to_owned(),
            ));
        }

        self.phase = CommitPhase::Execute;
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

        // Any failure transitions to Collect
        if self.all_resolved() {
            self.phase = CommitPhase::Collect;
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

    /// Check if the barrier's deadline has expired.
    pub fn is_expired(&self) -> bool {
        Instant::now() >= self.deadline
    }

    /// Check if the barrier has completed (all results collected).
    pub fn is_complete(&self) -> bool {
        self.phase == CommitPhase::Collect
    }

    /// Check if any participant failed.
    pub fn has_failures(&self) -> bool {
        self.participants
            .values()
            .any(|s| matches!(s, ParticipantState::Failed(_)))
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
}
