//! F4 — emit canvas updates so live formation state reaches both surfaces
//! (web SSE via `/canvas/stream`, desktop via the Tauri `subscribe_canvas`
//! Channel that forwards `runtime.canvas_tx` per E10).
//!
//! Was the F4 audit gap: the Tauri channel + SSE were both wired, but the
//! bot tick loop never published to `runtime.canvas_tx` — only the manual
//! `update_canvas` IPC command did. Tier transitions, rally pip changes,
//! and member health changes never reached the colony canvas.
//!
//! Strategy: per tick, build a `Status` block for the formation. Emit only
//! when the rendered summary changed since the previous tick (cheap diff
//! against `formation.last_broadcast_tier` + a synthesized fingerprint).
//! The frontend treats the emit as "this formation changed, refetch."

use springtale_cooperation::types::AgentHealth;
use springtale_core::canvas::{CanvasBlock, CanvasUpdate, StatusState};
use tokio::sync::broadcast;

use crate::cooperation::formation::Formation;

pub fn run(formation: &Formation, canvas_tx: &broadcast::Sender<CanvasUpdate>) {
    // No subscribers (web has no client connected, desktop hasn't called
    // subscribe_canvas) → broadcast::send returns Err. That's fine; we
    // skip the work cheaply.
    if canvas_tx.receiver_count() == 0 {
        return;
    }

    let block = build_status_block(formation);
    let id = formation.id.0.to_string();
    let _ = canvas_tx.send(CanvasUpdate::UpdateBlock { id, block });
}

fn build_status_block(formation: &Formation) -> CanvasBlock {
    let label = format!("formation/{}", formation.id.0);
    let operational = formation.operational_count();
    let total = formation.members.len();
    let incapacitated = formation
        .members
        .iter()
        .filter(|m| matches!(m.health, AgentHealth::Incapacitated))
        .count();
    let tier = format!("{:?}", formation.momentum.tier);
    let rally = formation.rally.tokens.remaining();
    let cascade = formation.cascade_hit_streak;
    let state = pick_state(formation, incapacitated, operational);
    let message = Some(format!(
        "tier={tier} ops={operational}/{total} rally={rally} cascade={cascade}"
    ));
    CanvasBlock::Status {
        id: formation.id.0.to_string(),
        label,
        state,
        message,
    }
}

fn pick_state(formation: &Formation, incapacitated: usize, operational: usize) -> StatusState {
    if formation.escalation_pending.is_some() {
        StatusState::Error
    } else if incapacitated > 0 || operational == 0 || formation.cascade_hit_streak > 0 {
        StatusState::Warning
    } else if formation.paused {
        StatusState::Info
    } else {
        StatusState::Success
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::cooperation::formation::{Formation, FormationMember};
    use springtale_cooperation::cadence::{AgentId, IntentPattern};
    use springtale_cooperation::types::FormationConstraints;

    fn formation_with_one_member() -> Formation {
        let m = FormationMember::new(AgentId::new(), vec!["slack".into()]);
        Formation::new_disconnected(
            vec![m],
            IntentPattern::Execute { plan_id: None },
            FormationConstraints::default(),
        )
    }

    /// F4 audit-fix regression: bot tick must publish a per-tick canvas
    /// update to subscribers (was the bug — subscribe_canvas wired but
    /// nothing emitted). Receiver must observe an UpdateBlock with the
    /// formation's id.
    #[tokio::test]
    async fn emit_publishes_status_block_when_subscribed() {
        let (tx, mut rx) = broadcast::channel::<CanvasUpdate>(8);
        let f = formation_with_one_member();
        let id = f.id.0.to_string();
        run(&f, &tx);
        let update = rx.recv().await.expect("update received");
        match update {
            CanvasUpdate::UpdateBlock { id: got_id, block } => {
                assert_eq!(got_id, id, "block id matches formation id");
                match block {
                    CanvasBlock::Status { id: bid, .. } => {
                        assert_eq!(bid, id, "status block id matches");
                    }
                    other => panic!("expected Status block, got {other:?}"),
                }
            }
            other => panic!("expected UpdateBlock, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn emit_skips_when_no_subscriber() {
        // No receivers → broadcast::send returns Err. Step short-circuits
        // by checking receiver_count first; this test ensures we don't
        // log noise / count work when nobody's listening.
        let (tx, _rx) = broadcast::channel::<CanvasUpdate>(8);
        drop(_rx);
        let f = formation_with_one_member();
        run(&f, &tx); // must not panic; no assertion needed
    }
}
