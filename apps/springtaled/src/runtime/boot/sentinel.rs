//! Daemon-startup sentinel checks (Phase-7 audit Finding B).
//!
//! Today this hosts the audit-log row-hash chain verifier. The
//! verifier walks `audit_trail` in `chain_seq` order, recomputes each
//! row's hash, and fails closed on any mismatch. The genesis anchor
//! is bound to the vault identity public key — so a fresh SQLite
//! with the same vault picks up where the previous chain left off, a
//! fresh SQLite with a different vault starts a new chain, and a
//! same SQLite + different vault triggers a vault-binding failure.

use anyhow::{Context, Result, bail};
use std::sync::Arc;

use springtale_crypto::identity::keypair::Keypair;
use springtale_sentinel::audit::verify::{VerifyError, verify_chain};
use springtale_store::StorageBackend;
use springtale_store::schema::audit_chain::vault_genesis_anchor;

/// Verify or stamp the audit-chain genesis anchor against the vault
/// identity, then run the row-hash chain verifier. Returns once the
/// chain is verified; logs and propagates errors so the daemon exits
/// non-zero on any tampering.
///
/// Invariants enforced:
/// 1. The vault identity → anchor mapping persists across restarts.
///    On first boot the anchor is written; on subsequent boots it
///    must match the current vault, else the chain belongs to a
///    different vault (a vault-rotation or tamper signal).
/// 2. Every row's `row_hash` recomputes to its stored value.
/// 3. `chain_seq` is strictly monotonic starting at 1.
pub(super) async fn verify_audit_chain(
    store: &Arc<dyn StorageBackend>,
    keypair: &Keypair,
) -> Result<()> {
    let pub_bytes = keypair.verifying_key().to_bytes();
    let anchor = vault_genesis_anchor(&pub_bytes);

    let stored = store
        .get_config("audit.chain.anchor")
        .await
        .context("failed to read audit chain anchor from config_store")?;

    match stored {
        Some(raw) => {
            // The anchor JSON-encodes its String value via the generic
            // KV path. Tolerate the legacy raw-hex form too.
            let observed: String = serde_json::from_str(&raw).unwrap_or(raw);
            if observed != anchor {
                bail!(
                    "audit chain anchor mismatch — current vault identity does not match \
                     the vault that originally stamped the chain. This means either the \
                     vault was rotated or the audit_trail belongs to a different daemon \
                     install. Aborting to preserve forensic integrity."
                );
            }
        }
        None => {
            let encoded =
                serde_json::to_string(&anchor).context("failed to serialize audit chain anchor")?;
            store
                .set_config("audit.chain.anchor", &encoded)
                .await
                .context("failed to stamp audit chain anchor in config_store")?;
            tracing::info!(
                anchor_prefix = %&anchor[..16],
                "audit chain genesis anchor stamped to vault identity"
            );
        }
    }

    match verify_chain(store, &anchor).await {
        Ok(ok) => {
            tracing::info!(
                rows_verified = ok.rows_verified,
                tip_hash_prefix = %&ok.tip_hash[..ok.tip_hash.len().min(16)],
                "audit chain verified"
            );
            Ok(())
        }
        Err(VerifyError::ChainBroken(b)) => {
            tracing::error!(
                row_id = %b.row_id,
                chain_seq = b.chain_seq,
                reason = ?b.reason,
                expected = %b.expected,
                observed = %b.observed,
                "AUDIT CHAIN BROKEN — refusing to start"
            );
            Err(anyhow::anyhow!(
                "audit chain broken at chain_seq {} (row id {}): {:?}",
                b.chain_seq,
                b.row_id,
                b.reason
            ))
        }
        Err(VerifyError::Store(e)) => {
            Err(anyhow::Error::new(e).context("audit chain verification failed"))
        }
    }
}
