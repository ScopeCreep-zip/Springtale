//! Cooperation events — typed taxonomy + envelope for the
//! `CooperationEventEnvelope` broadcast stream (Phase H).
//!
//! The bus itself is owned by `springtale-runtime::RuntimeState` (next to
//! `canvas_tx` per H2); this module owns the event type definitions only.
//! Subscribers read via the SSE endpoint `/cooperation/events` (web,
//! Phase H3) or the Tauri `subscribe_cooperation` IPC channel
//! (desktop, Phase H4).

pub mod types;

pub use types::{
    CooperationEvent, CooperationEventEnvelope, InterferenceKind, InterventionKind,
    ReplanOutcomeSummary, VoteOutcome,
};

use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::broadcast;

/// Process-wide monotonic event sequence. Frontend uses to detect missed
/// envelopes (e.g., after a lag gap in `BroadcastStream::filter_map`).
static EVENT_SEQ: AtomicU64 = AtomicU64::new(0);

/// Send a cooperation event through the broadcast bus. Builds the envelope
/// (seq + UTC timestamp) and ignores send errors when nobody's listening.
///
/// Optional sender — `None` means headless / test mode (matches the
/// `canvas_tx: Option<...>` precedent in `TickDeps`).
pub fn emit(
    sender: Option<&broadcast::Sender<CooperationEventEnvelope>>,
    event: CooperationEvent,
) {
    let Some(tx) = sender else {
        return;
    };
    if tx.receiver_count() == 0 {
        return;
    }
    let envelope = CooperationEventEnvelope {
        seq: EVENT_SEQ.fetch_add(1, Ordering::Relaxed),
        at: chrono::Utc::now(),
        event,
    };
    let _ = tx.send(envelope);
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::cadence::AgentId;
    use crate::types::FormationId;

    #[test]
    fn emit_with_no_subscribers_is_noop() {
        let (tx, rx) = broadcast::channel::<CooperationEventEnvelope>(8);
        drop(rx); // zero subscribers
        emit(
            Some(&tx),
            CooperationEvent::CascadeHit {
                formation_id: FormationId::new(),
                streak: 1,
                members_affected: 1,
            },
        );
        // No panic, no error — short-circuit on receiver_count == 0.
    }

    #[tokio::test]
    async fn emit_increments_seq_per_event() {
        let (tx, mut rx) = broadcast::channel::<CooperationEventEnvelope>(8);
        let agent = AgentId::new();
        for _ in 0..3 {
            emit(
                Some(&tx),
                CooperationEvent::SacrificeYield {
                    formation_id: FormationId::new(),
                    sacrificer: agent,
                    beneficiary: AgentId::new(),
                    utility: 0.7,
                },
            );
        }
        let a = rx.recv().await.unwrap();
        let b = rx.recv().await.unwrap();
        let c = rx.recv().await.unwrap();
        assert!(b.seq > a.seq);
        assert!(c.seq > b.seq);
    }

    #[test]
    fn emit_with_none_sender_is_noop() {
        // Headless / test path — TickDeps.cooperation_tx is None.
        emit(
            None,
            CooperationEvent::SupervisorEscalated {
                formation_id: FormationId::new(),
                reason: "test".into(),
            },
        );
    }
}
