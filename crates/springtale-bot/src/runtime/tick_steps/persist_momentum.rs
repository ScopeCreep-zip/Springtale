//! Step 5 — persist the momentum row to the dedicated `formation_momentum`
//! table so tier + counters survive restarts (`COOPERATION.md §7`).
//!
//! Failure to persist is logged but does not abort the tick; the in-memory
//! state continues to evolve and the next successful tick will catch up.

use crate::cooperation::formation::Formation;
use springtale_store::{FormationMomentumRow, StorageBackend};

pub async fn run(store: &dyn StorageBackend, formation: &Formation) {
    let row = FormationMomentumRow {
        formation_id: formation.id.0.to_string(),
        tier: format!("{:?}", formation.momentum.tier),
        consecutive_successes: formation.momentum.consecutive_successes as i64,
        // The `interference_count` column holds the lifetime total. The
        // per-run counter is always 0 between events and is never stored.
        interference_count: formation.momentum.interference_total as i64,
        updated_at: chrono::Utc::now(),
    };
    if let Err(e) = store.upsert_formation_momentum(&row).await {
        tracing::warn!(
            formation_id = %formation.id.0,
            error = %e,
            "failed to persist momentum"
        );
    }
}
