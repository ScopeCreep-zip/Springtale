//! Plan §1.15 F — a bounded, newest-first ring of utterances so polling
//! frontends (`GET /cooperation/utterances/recent`) see what streaming
//! ones see. Filled by a collector task subscribed to `cooperation_tx`.

use std::collections::VecDeque;
use std::sync::Arc;

use tokio::sync::{RwLock, broadcast};

use springtale_cooperation::events::{CooperationEvent, CooperationEventEnvelope};
use springtale_cooperation::utterance::Utterance;

/// Ring capacity. Oldest entries fall off the back.
pub const UTTERANCE_RING_CAP: usize = 1000;

/// Newest-first ring shared between the collector and readers.
pub type UtteranceRing = Arc<RwLock<VecDeque<Utterance>>>;

/// Push `u` to the front, dropping the oldest entry at capacity.
pub fn push(ring: &mut VecDeque<Utterance>, u: Utterance) {
    if ring.len() >= UTTERANCE_RING_CAP {
        ring.pop_back();
    }
    ring.push_front(u);
}

/// Spawn the collector: every `CooperationEvent::Utterance` on `rx` lands
/// at the front of `ring`. Lag is logged and skipped; close ends the task.
pub fn spawn_collector(
    ring: UtteranceRing,
    mut rx: broadcast::Receiver<CooperationEventEnvelope>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(env) => {
                    if matches!(env.event, CooperationEvent::Utterance { .. })
                        && let Ok(u) = Utterance::try_from(env.event)
                    {
                        push(&mut *ring.write().await, u);
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "utterance ring collector lagged");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    })
}

/// Snapshot of the ring, newest first.
pub async fn recent(ring: &UtteranceRing) -> Vec<Utterance> {
    ring.read().await.iter().cloned().collect()
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use springtale_cooperation::TickId;
    use springtale_cooperation::utterance::{UtteranceDefs, UtteranceKind, emit_solo};
    use springtale_core::rule::RuleId;

    fn rule() -> RuleId {
        RuleId(uuid::Uuid::nil())
    }

    #[tokio::test]
    async fn test_utterance_ring_emitted_appears_at_front() {
        let (tx, _keep) = broadcast::channel(16);
        let ring = UtteranceRing::default();
        let _collector = spawn_collector(ring.clone(), tx.subscribe());
        let defs = UtteranceDefs::default();
        emit_solo(Some(&tx), &defs, rule(), TickId(1), UtteranceKind::Firing);
        emit_solo(Some(&tx), &defs, rule(), TickId(2), UtteranceKind::Failed);
        for _ in 0..100 {
            if ring.read().await.len() == 2 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let snapshot = recent(&ring).await;
        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot[0].seq, TickId(2));
        assert_eq!(snapshot[0].utterance, UtteranceKind::Failed);
        assert_eq!(snapshot[0].rule_id, Some(rule()));
        assert_eq!(snapshot[0].formation_id, None);
    }

    #[test]
    fn test_utterance_ring_cap_holds() {
        let defs = UtteranceDefs::default();
        let mut ring = VecDeque::new();
        let total = u64::try_from(UTTERANCE_RING_CAP).expect("cap fits u64") + 1;
        for i in 1..=total {
            let u = emit_solo(None, &defs, rule(), TickId(i), UtteranceKind::Firing)
                .expect("firing has a def");
            push(&mut ring, u);
        }
        assert_eq!(ring.len(), UTTERANCE_RING_CAP);
        assert_eq!(ring.front().map(|u| u.seq), Some(TickId(total)));
        assert_eq!(ring.back().map(|u| u.seq), Some(TickId(2)));
    }
}
