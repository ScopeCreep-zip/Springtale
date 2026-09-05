//! Restore formations persisted from a previous run (plan 6.11 / finding 119).
//!
//! `spawn_formation` (bot event loop, `springtale-bot::cooperation::lifecycle`)
//! builds a live `Formation` from a stored row and its member rows, including
//! the 1.13 model load — but nothing called it at boot, so a restart left
//! every previously-active formation dark until a client re-deployed it by
//! hand. This module re-issues the same `FormationCommand`s the API would
//! send, sourced from what's already on disk.

use std::sync::Arc;

use anyhow::Result;
use tokio::sync::mpsc::Sender;

use springtale_cooperation::command::FormationCommand;
use springtale_cooperation::types::FormationId;
use springtale_store::StorageBackend;
use springtale_store::schema::formations::{STATUS_ACTIVE, STATUS_PAUSED};

/// Re-deploy every stored formation that was `active` or `paused` when the
/// daemon last stopped, in row order: `Deploy` for both, then `Pause` for
/// the ones that were paused. The bot's existing command handling does the
/// rest (materializing the live `Formation`, loading the mental model).
///
/// A formation whose connectors are not installed still restores — it
/// spawns with missing capabilities and reports that through the normal
/// liveness path, it is not skipped. Only a row whose id fails to parse as
/// a `FormationId` is skipped (logged at `warn`); every other row still
/// restores.
///
/// Called after `init_bot` returns, so the bot event loop already owns
/// `formation_cmd_rx` and is draining it — sends here simply queue behind
/// whatever's already in flight, they don't block boot on a full channel.
pub(crate) async fn restore_formations(
    store: &Arc<dyn StorageBackend>,
    tx: &Sender<FormationCommand>,
) -> Result<usize> {
    let rows = store.list_formations().await?;
    let mut restored = 0usize;

    for row in rows {
        if row.status != STATUS_ACTIVE && row.status != STATUS_PAUSED {
            continue;
        }

        let formation_id = match FormationId::parse(&row.id) {
            Ok(id) => id,
            Err(error) => {
                tracing::warn!(
                    formation_id = %row.id,
                    %error,
                    "skipping formation with unparsable id while restoring at boot"
                );
                continue;
            }
        };

        if tx
            .send(FormationCommand::Deploy { formation_id })
            .await
            .is_err()
        {
            tracing::warn!("formation command channel closed while restoring formations at boot");
            break;
        }

        if row.status == STATUS_PAUSED
            && tx
                .send(FormationCommand::Pause { formation_id })
                .await
                .is_err()
        {
            tracing::warn!("formation command channel closed while restoring formations at boot");
            break;
        }

        restored += 1;
    }

    tracing::info!(restored, "restored persisted formations at boot");
    Ok(restored)
}
