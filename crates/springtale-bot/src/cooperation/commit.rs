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
//! Cooperation is in planning; execution is deterministic.

use std::time::Duration;

/// Phases of a synchronized commit operation.
///
/// From COOPERATION.pdf §12:
/// ```text
/// pub enum CommitPhase {
///     Prepare,
///     Ready,
///     Countdown { remaining: Duration },
///     Execute,
///     Collect,
/// }
/// ```
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_commit_phase_lifecycle() {
        // Verify the state machine compiles and variants exist
        let _p = CommitPhase::Prepare;
        let _r = CommitPhase::Ready;
        let _c = CommitPhase::Countdown {
            remaining: Duration::from_secs(3),
        };
        let _e = CommitPhase::Execute;
        let _co = CommitPhase::Collect;
    }
}
