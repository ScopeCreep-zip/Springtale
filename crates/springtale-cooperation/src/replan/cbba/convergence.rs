//! Convergence driver — iterate the consensus round until no changes.
//!
//! In the multi-agent case this is just the termination check: every agent
//! calls `consensus::round` against each neighbor, and the round system
//! halts when every pair reports `Converged` in the same sweep.

use super::types::ConvergenceStatus;

/// Fold a sequence of per-neighbor statuses into the formation-wide status
/// for one sweep. Returns `Running` if anybody changed; `Converged` only when
/// every pairwise exchange was silent.
pub fn fold_sweep(statuses: &[ConvergenceStatus]) -> ConvergenceStatus {
    if statuses
        .iter()
        .all(|s| matches!(s, ConvergenceStatus::Converged))
    {
        ConvergenceStatus::Converged
    } else if statuses
        .iter()
        .any(|s| matches!(s, ConvergenceStatus::Stalled))
    {
        ConvergenceStatus::Stalled
    } else {
        ConvergenceStatus::Running
    }
}

/// Declare the round stalled when the iteration count exceeds `max_rounds`.
/// CBBA's theoretical bound is `min(N_agents, N_tasks) · diameter(network)`;
/// for our fully-connected single-process case, stall just means a bug.
pub fn check_stall(iteration: u32, max_rounds: u32) -> ConvergenceStatus {
    if iteration >= max_rounds {
        ConvergenceStatus::Stalled
    } else {
        ConvergenceStatus::Running
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn all_converged_reports_converged() {
        let s = fold_sweep(&[ConvergenceStatus::Converged, ConvergenceStatus::Converged]);
        assert_eq!(s, ConvergenceStatus::Converged);
    }

    #[test]
    fn any_running_reports_running() {
        let s = fold_sweep(&[ConvergenceStatus::Converged, ConvergenceStatus::Running]);
        assert_eq!(s, ConvergenceStatus::Running);
    }

    #[test]
    fn any_stalled_reports_stalled() {
        let s = fold_sweep(&[ConvergenceStatus::Converged, ConvergenceStatus::Stalled]);
        assert_eq!(s, ConvergenceStatus::Stalled);
    }

    #[test]
    fn stall_triggers_at_max_rounds() {
        assert_eq!(check_stall(10, 10), ConvergenceStatus::Stalled);
        assert_eq!(check_stall(9, 10), ConvergenceStatus::Running);
    }
}
