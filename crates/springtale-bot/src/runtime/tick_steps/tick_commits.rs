//! Step 11b — drive synchronized-commit barrier phase transitions (§12).
//!
//! Each tick, calls `CommitBarrier::tick(now)` for every active barrier on
//! the formation. Transitions returned by `tick()` map directly onto
//! `CooperationEvent::CommitPhaseChanged` envelopes (Phase H5) so the UI
//! event ribbon + formation log surface the synchronized-commit lifecycle
//! alongside the rest of cooperation state.
//!
//! Runs **before** `expire_commits` so:
//!   1. `tick_commits` advances each barrier (Prepare→Ready→Execute→Collect
//!      or → Aborted).
//!   2. `expire_commits` drops barriers that are now terminal.
//!
//! Per §12 fail-fast semantics, prepare-phase failures don't wait for the
//! deadline — they abort the barrier immediately. The driver here only
//! handles deadline-driven and auto-advance transitions; explicit
//! abort-on-prepare-failure happens at the caller site via
//! `CommitBarrier::record_prepare_failure`.

use std::time::Instant;

use tokio::sync::broadcast;

use crate::cooperation::formation::Formation;
use springtale_cooperation::events::{self, CooperationEvent, CooperationEventEnvelope};

pub fn run(
    formation: &mut Formation,
    cooperation_tx: Option<&broadcast::Sender<CooperationEventEnvelope>>,
) {
    let now = Instant::now();
    for barrier in formation.active_commits.iter_mut() {
        if let Some(transition) = barrier.tick(now) {
            events::emit(
                cooperation_tx,
                CooperationEvent::CommitPhaseChanged {
                    formation_id: formation.id,
                    barrier_id: barrier.id,
                    phase: transition.to,
                },
            );
            tracing::debug!(
                formation = %formation.id.0,
                barrier = %barrier.id,
                from = transition.from,
                to = transition.to,
                "commit barrier phase advanced",
            );
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::cooperation::formation::{Formation, FormationMember};
    use springtale_cooperation::cadence::{AgentId, IntentPattern};
    use springtale_cooperation::capability::CapabilityDecl;
    use springtale_cooperation::types::{AgentHealth, FormationConstraints, FuelAmount};
    use std::time::Duration;

    fn make_formation(member_count: usize) -> (Formation, Vec<AgentId>) {
        let mut members = Vec::new();
        let mut ids = Vec::new();
        for _ in 0..member_count {
            let id = AgentId::new();
            let mut m = FormationMember::new(id, vec![CapabilityDecl::new("slack")]);
            m.health = AgentHealth::Operational;
            ids.push(id);
            members.push(m);
        }
        let f = Formation::new_disconnected(
            members,
            IntentPattern::Execute { plan_id: None },
            FormationConstraints {
                fuel_budget: FuelAmount(1_000_000),
                ..Default::default()
            },
        );
        (f, ids)
    }

    #[tokio::test]
    async fn tick_aborts_prepare_on_deadline() {
        let (mut f, ids) = make_formation(2);
        // Open a barrier with an already-expired deadline.
        f.begin_commit(&ids, Duration::from_millis(0), ids[0]);
        let (tx, mut rx) = broadcast::channel(8);

        // Sleep beyond the 0ms deadline so `Instant::now()` is past.
        std::thread::sleep(Duration::from_millis(2));
        run(&mut f, Some(&tx));

        let envelope = rx.try_recv().unwrap();
        match envelope.event {
            CooperationEvent::CommitPhaseChanged { phase, .. } => {
                assert_eq!(phase, "aborted");
            }
            _ => panic!("expected CommitPhaseChanged"),
        }
        assert!(f.active_commits[0].was_aborted());
    }

    #[tokio::test]
    async fn tick_advances_ready_to_execute() {
        let (mut f, ids) = make_formation(2);
        let bid = f.begin_commit(&ids, Duration::from_secs(30), ids[0]);
        f.signal_commit_ready(bid, ids[0]).unwrap();
        f.signal_commit_ready(bid, ids[1]).unwrap();
        let (tx, mut rx) = broadcast::channel(8);

        run(&mut f, Some(&tx));

        let envelope = rx.try_recv().unwrap();
        match envelope.event {
            CooperationEvent::CommitPhaseChanged { phase, .. } => {
                assert_eq!(phase, "execute");
            }
            _ => panic!("expected CommitPhaseChanged"),
        }
    }
}
