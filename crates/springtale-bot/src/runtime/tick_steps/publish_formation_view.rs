//! G6 — broadcast this formation's running-state snapshot on the
//! cross-formation gossip bus once per cooperation tick.
//!
//! The published `FormationView` is small (intent + tier + counts +
//! status) — peer formations use it as soft-state input to their own
//! decisions, not as a state-replication mechanism. Per Quickwit's
//! chitchat scuttlebutt design, only the owning node writes its own
//! keys; peers read everyone else's writes via the bus's `subscribe()`
//! channel.
//!
//! Runs near the tail of the tick so the published view reflects the
//! decisions every prior step made (momentum updates, supervision,
//! pacing transitions, etc.). Skipped when the formation has no gossip
//! bus wired (CLI / test contexts).

use std::sync::Arc;

use crate::cooperation::formation::Formation;
use springtale_cooperation::gossip::{
    FormationGossipBus, FormationStatus, FormationView,
};

pub fn run(formation: &Formation) {
    let Some(bus) = formation.formation_gossip.clone() else {
        return;
    };
    let view = build_view(formation);
    spawn_publish(bus, view);
}

fn build_view(formation: &Formation) -> FormationView {
    FormationView {
        formation_id: formation.id,
        intent: formation.intent.clone(),
        momentum_tier: formation.momentum.tier,
        operational_count: formation.operational_count() as u32,
        member_count: formation.members.len() as u32,
        rally_tokens_remaining: formation.rally.tokens.remaining() as u32,
        status: derive_status(formation),
        at: chrono::Utc::now(),
    }
}

fn derive_status(formation: &Formation) -> FormationStatus {
    if formation.paused {
        FormationStatus::Paused
    } else if !formation.is_viable() {
        FormationStatus::Dissolved
    } else {
        FormationStatus::Active
    }
}

fn spawn_publish(bus: Arc<dyn FormationGossipBus>, view: FormationView) {
    tokio::spawn(async move {
        bus.publish_view(view).await;
    });
}
