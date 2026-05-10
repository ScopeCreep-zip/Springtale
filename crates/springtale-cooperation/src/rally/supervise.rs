//! Rally supervisor — drains `FormationRally::members` and emits
//! `RallyEvent`s as member tasks exit.
//!
//! Per COOPERATION.md §15.2: the supervisor is the failure detector.
//! Any task that panics, errors out, or returns `AgentOutcome::Failed`
//! produces a `RallyEvent::PeerDown`. Planned leaves (via
//! `FormationRally::leave`) surface as `JoinError::is_cancelled()` and
//! are logged at debug level — no token consumed, no event emitted.
//!
//! Two supervisor styles:
//! - [`run`] — async loop; awaits `join_next` until the JoinSet drains.
//!   Appropriate when the supervisor is a dedicated task.
//! - [`drain`] — synchronous non-blocking pass over `try_join_next`.
//!   Appropriate when the formation's tick processor wants supervision
//!   folded into the per-tick path without a separate task (avoids
//!   an Arc<Mutex<JoinSet>> dance while keeping the same semantics).

use tokio::task::JoinError;

use super::types::{AgentOutcome, FailureReason, FormationRally, RallyEvent};

/// Run until every supervised member has exited. Exits when the
/// `JoinSet` is empty, which on a live formation only happens at
/// dissolve time.
pub async fn run(rally: &mut FormationRally) {
    while let Some(result) = rally.members.join_next().await {
        handle_outcome(result, rally);
    }
    tracing::debug!("rally supervisor: join set drained, exiting");
}

/// Non-blocking supervision pass. Drains every outcome that's ready
/// right now and returns the count handled. Designed to be called from
/// the tick processor so supervision runs on the same cadence as the
/// rest of the formation — no dedicated task required.
pub fn drain(rally: &mut FormationRally) -> usize {
    let mut handled = 0usize;
    while let Some(result) = rally.members.try_join_next() {
        handle_outcome(result, rally);
        handled += 1;
    }
    handled
}

fn handle_outcome(result: Result<AgentOutcome, JoinError>, rally: &FormationRally) {
    match result {
        Ok(AgentOutcome::CleanExit { agent }) => {
            // Planned shutdown — no token consumed, no event emitted.
            tracing::debug!(?agent, "rally supervisor: clean exit");
        }
        Ok(AgentOutcome::Failed { agent, reason }) => {
            tracing::warn!(?agent, ?reason, "rally supervisor: member failed");
            let _ = rally.events.send(RallyEvent::PeerDown { agent });
        }
        Err(e) if e.is_cancelled() => {
            // Planned leave via FormationRally::leave — no token
            // consumed, no event emitted.
            tracing::debug!("rally supervisor: member cancelled");
        }
        Err(e) if e.is_panic() => {
            tracing::error!("rally supervisor: member panicked");
            let _ = rally.events.send(RallyEvent::Escalated {
                reason: "member task panicked".to_owned(),
            });
        }
        Err(e) => {
            tracing::error!(error = %e, "rally supervisor: unexpected join error");
        }
    }
}

/// Construct a `Failed` outcome that a member task can yield to trigger
/// rally without needing to build a `FailureReason` manually.
pub fn failed(agent: crate::cadence::AgentId, reason: FailureReason) -> AgentOutcome {
    AgentOutcome::Failed { agent, reason }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::super::types::{FormationRally, RallyEvent};
    use super::*;
    use crate::cadence::AgentId;

    #[tokio::test]
    async fn failed_member_produces_peer_down_event() {
        let mut rally = FormationRally::new(3, 8);
        let agent = AgentId::new();
        let mut events = rally.subscribe();

        rally.spawn_member(agent, async move {
            AgentOutcome::Failed {
                agent,
                reason: FailureReason::CapabilityExhausted,
            }
        });

        run(&mut rally).await;

        // At least one PeerDown event for our agent.
        let ev = events.try_recv().expect("expected PeerDown event");
        assert!(matches!(ev, RallyEvent::PeerDown { agent: a } if a == agent));
    }

    #[tokio::test]
    async fn clean_exit_produces_no_event() {
        let mut rally = FormationRally::new(3, 8);
        let agent = AgentId::new();
        let mut events = rally.subscribe();

        rally.spawn_member(agent, async move { AgentOutcome::CleanExit { agent } });
        run(&mut rally).await;

        assert!(events.try_recv().is_err());
    }
}
