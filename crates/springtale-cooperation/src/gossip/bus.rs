//! Single-process `FormationGossipBus` — `tokio::sync::broadcast` fan-out
//! plus a `DashMap` of last-known views per formation.
//!
//! When a real chitchat-backed bus lands (one node per springtaled
//! process) it implements the same trait; this in-memory version remains
//! the test fixture and the default when chitchat seeding is disabled.
//!
//! Subscribers hand in their own `FormationId` so the bus filters its
//! own deltas out of the receiver — a formation that called
//! `publish_view(view)` does not receive `View(view)` back on its own
//! `subscribe()` channel. This lets the tick-loop publish every tick
//! without spamming itself.

use async_trait::async_trait;
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::broadcast;

use super::trait_::FormationGossipBus;
use super::types::{FormationDelta, FormationOutcome, FormationView};
use crate::types::FormationId;

/// Channel capacity for the broadcast fan-out. Sized for ~16 formations
/// each emitting at the cooperation tick rate (≤30 Hz) with a 30s
/// subscriber lag budget.
const CHANNEL_CAPACITY: usize = 1024;

pub struct InMemoryFormationGossipBus {
    views: DashMap<FormationId, FormationView>,
    outcomes: DashMap<FormationId, FormationOutcome>,
    tx: broadcast::Sender<FormationDelta>,
}

impl InMemoryFormationGossipBus {
    pub fn new() -> Arc<Self> {
        let (tx, _rx) = broadcast::channel(CHANNEL_CAPACITY);
        Arc::new(Self {
            views: DashMap::new(),
            outcomes: DashMap::new(),
            tx,
        })
    }
}

impl Default for InMemoryFormationGossipBus {
    fn default() -> Self {
        let (tx, _rx) = broadcast::channel(CHANNEL_CAPACITY);
        Self {
            views: DashMap::new(),
            outcomes: DashMap::new(),
            tx,
        }
    }
}

#[async_trait]
impl FormationGossipBus for InMemoryFormationGossipBus {
    async fn publish_view(&self, view: FormationView) {
        self.views.insert(view.formation_id, view.clone());
        // `send` errors when there are no subscribers — fine, drop.
        let _ = self.tx.send(FormationDelta::View(view));
    }

    async fn publish_outcome(&self, outcome: FormationOutcome) {
        self.outcomes.insert(outcome.formation_id, outcome.clone());
        let _ = self.tx.send(FormationDelta::Outcome(outcome));
    }

    async fn snapshot(&self) -> Vec<FormationView> {
        self.views.iter().map(|e| e.value().clone()).collect()
    }

    fn subscribe(&self, excluding: FormationId) -> broadcast::Receiver<FormationDelta> {
        // Wrap the raw broadcast receiver in a filtering proxy so the
        // caller never sees its own deltas.
        let (filtered_tx, filtered_rx) = broadcast::channel(CHANNEL_CAPACITY);
        let mut source = self.tx.subscribe();
        // Replay every existing outcome immediately so a late subscriber
        // (e.g. a formation spawned after others have already finished)
        // still sees historical context. Views are *not* replayed because
        // they're churn data — subscribers will get the next tick's view
        // anyway.
        for entry in self.outcomes.iter() {
            if entry.key() != &excluding {
                let _ = filtered_tx.send(FormationDelta::Outcome(entry.value().clone()));
            }
        }
        tokio::spawn(async move {
            while let Ok(delta) = source.recv().await {
                if delta.formation_id() != excluding && filtered_tx.send(delta).is_err() {
                    // No live subscribers on the filtered side — bail.
                    break;
                }
            }
        });
        filtered_rx
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::cadence::IntentPattern;
    use crate::gossip::types::FormationStatus;
    use crate::momentum::MomentumTier;

    fn view(id: FormationId, intent: IntentPattern) -> FormationView {
        FormationView {
            formation_id: id,
            intent,
            momentum_tier: MomentumTier::Cold,
            operational_count: 1,
            member_count: 1,
            rally_tokens_remaining: 3,
            status: FormationStatus::Active,
            at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn publish_and_snapshot_round_trip() {
        let bus = InMemoryFormationGossipBus::new();
        let id = FormationId::new();
        bus.publish_view(view(id, IntentPattern::Execute { plan_id: None }))
            .await;
        let snap = bus.snapshot().await;
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].formation_id, id);
    }

    #[tokio::test]
    async fn subscribe_filters_own_formation() {
        let bus = InMemoryFormationGossipBus::new();
        let me = FormationId::new();
        let peer = FormationId::new();
        let mut rx = bus.subscribe(me);
        // Give the spawn a moment to wire up.
        tokio::task::yield_now().await;

        bus.publish_view(view(peer, IntentPattern::Execute { plan_id: None }))
            .await;
        bus.publish_view(view(me, IntentPattern::Execute { plan_id: None }))
            .await;

        // Use a short timeout — first event must be the peer's, and the
        // self-published view must be filtered out.
        let first = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv())
            .await
            .expect("expected at least one delta")
            .expect("recv ok");
        assert_eq!(first.formation_id(), peer);

        // No second event arrives within the window — own view filtered.
        let second = tokio::time::timeout(std::time::Duration::from_millis(80), rx.recv()).await;
        assert!(second.is_err(), "self-view should not arrive: {second:?}");
    }

    #[tokio::test]
    async fn outcomes_replay_for_late_subscribers() {
        let bus = InMemoryFormationGossipBus::new();
        let peer = FormationId::new();
        bus.publish_outcome(FormationOutcome {
            formation_id: peer,
            final_intent: IntentPattern::Execute { plan_id: None },
            success_count: 5,
            failure_count: 0,
            dissolve_reason: "complete".into(),
            at: chrono::Utc::now(),
        })
        .await;

        let me = FormationId::new();
        let mut rx = bus.subscribe(me);
        let first = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv())
            .await
            .expect("late subscriber sees outcome replay")
            .expect("recv ok");
        assert!(matches!(first, FormationDelta::Outcome(_)));
        assert_eq!(first.formation_id(), peer);
    }
}
