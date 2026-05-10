//! Graceful-shutdown snapshot logging.
//!
//! When the event loop's `select!` falls through (all channels closed), we
//! log one structured event per active formation so operators can tell
//! whether the bot exited quietly or mid-work. Formation `Drop` impls abort
//! their dispatcher tasks; rally supervision is in-tick (no task to abort);
//! `SwimNode` (if any) is owned by `RuntimeState`, not by the `Bot`, so it
//! survives bot shutdown by design.

use crate::runtime::lifecycle::Bot;

pub async fn log_shutdown_snapshot(bot: &Bot) {
    let snapshot = {
        let formations = bot.formations.read().await;
        formations
            .iter()
            .map(|f| {
                (
                    f.id.0.to_string(),
                    f.members.len(),
                    f.operational_count(),
                    f.rally.tokens.remaining() as u32,
                    format!("{:?}", f.momentum.tier),
                )
            })
            .collect::<Vec<_>>()
    };
    tracing::info!(formations_active = snapshot.len(), "bot event loop drained");
    for (id, members, operational, rally_remaining, tier) in &snapshot {
        tracing::info!(
            formation = %id,
            members = members,
            operational = operational,
            rally_remaining = rally_remaining,
            tier = %tier,
            "formation at shutdown"
        );
    }
}
