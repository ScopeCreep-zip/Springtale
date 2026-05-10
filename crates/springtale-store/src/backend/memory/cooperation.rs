//! In-memory backend for cooperation primitives — CAS writes, environment
//! deposits, and shared mental model. All operations use `tokio::sync::RwLock`
//! maps so the trait methods stay `async`.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::StoreError;
use crate::schema::cooperation::{CoopCasOutcome, CoopDepositRow};
use crate::schema::mental_model::MentalModelBundle;

use super::InMemoryBackend;

#[derive(Debug, Clone)]
pub struct CoopWriteEntry {
    pub value: Vec<u8>,
    pub writer: String,
    pub tick: i64,
}

/// Tracks an in-memory environment-mediated deposit. Every field is
/// read by `coop_list_deposits` (observability / canvas UI "handoff_ready"
/// surface, per spec §20.3).
#[derive(Debug, Clone)]
pub struct CoopDepositEntry {
    pub payload: Vec<u8>,
    pub depositor: String,
    pub deposited_at: i64,
    pub expires_at: Option<i64>,
    pub claimed_by: Option<String>,
}

impl InMemoryBackend {
    pub(super) async fn coop_cas_write_impl(
        &self,
        tick: i64,
        writer: &str,
        key: &str,
        expected: Option<&[u8]>,
        proposed: &[u8],
    ) -> Result<CoopCasOutcome, StoreError> {
        let mut guard = self.coop_writes.write().await;
        let current = guard.get(key).cloned();
        let matches = match (expected, &current) {
            (None, None) => true,
            (Some(e), Some(entry)) => e == entry.value.as_slice(),
            _ => false,
        };
        if !matches {
            return Ok(match current {
                Some(entry) => CoopCasOutcome::Mismatch {
                    current_value: entry.value,
                    current_writer: entry.writer,
                    current_tick: entry.tick,
                },
                None => CoopCasOutcome::Mismatch {
                    current_value: Vec::new(),
                    current_writer: String::new(),
                    current_tick: 0,
                },
            });
        }
        guard.insert(
            key.to_owned(),
            CoopWriteEntry {
                value: proposed.to_vec(),
                writer: writer.to_owned(),
                tick,
            },
        );
        Ok(CoopCasOutcome::Applied)
    }

    pub(super) async fn coop_deposit_impl(
        &self,
        location: &str,
        payload: &[u8],
        depositor: &str,
        ttl_secs: Option<i64>,
    ) -> Result<(), StoreError> {
        let now = unix_now();
        let expires_at = ttl_secs.map(|t| now + t);
        self.coop_deposits.write().await.insert(
            location.to_owned(),
            CoopDepositEntry {
                payload: payload.to_vec(),
                depositor: depositor.to_owned(),
                deposited_at: now,
                expires_at,
                claimed_by: None,
            },
        );
        Ok(())
    }

    pub(super) async fn coop_collect_impl(
        &self,
        location: &str,
        collector: &str,
    ) -> Result<Option<Vec<u8>>, StoreError> {
        let mut guard = self.coop_deposits.write().await;
        let Some(entry) = guard.get(location) else {
            return Ok(None);
        };
        if entry.claimed_by.is_some() {
            return Ok(None);
        }
        let payload = entry.payload.clone();
        // Mirror the SQLite semantic: record the claimant in the audit
        // trail, then remove the deposit. The claimed_by field in
        // CoopDepositEntry is observed by `coop_list_deposits` on the
        // SQLite side; in-memory does the same for parity even though
        // the entry is about to be dropped.
        tracing::debug!(
            location = %location,
            collector = %collector,
            depositor = %entry.depositor,
            "coop deposit claimed (in-memory)"
        );
        guard.remove(location);
        Ok(Some(payload))
    }

    pub(super) async fn coop_sweep_expired_impl(&self) -> Result<u64, StoreError> {
        let now = unix_now();
        let mut guard = self.coop_deposits.write().await;
        let before = guard.len();
        guard.retain(|_, entry| entry.expires_at.is_none_or(|exp| exp >= now));
        Ok((before - guard.len()) as u64)
    }

    pub(super) async fn coop_list_deposits_impl(&self) -> Result<Vec<CoopDepositRow>, StoreError> {
        Ok(self
            .coop_deposits
            .read()
            .await
            .iter()
            .map(|(loc, e)| CoopDepositRow {
                location: loc.clone(),
                payload: e.payload.clone(),
                depositor: e.depositor.clone(),
                deposited_at: e.deposited_at,
                expires_at: e.expires_at,
                claimed_by: e.claimed_by.clone(),
            })
            .collect())
    }

    pub(super) async fn mental_model_save_impl(
        &self,
        formation_id: &str,
        bundle: &MentalModelBundle,
    ) -> Result<(), StoreError> {
        self.mental_model
            .write()
            .await
            .insert(formation_id.to_owned(), bundle.clone());
        Ok(())
    }

    pub(super) async fn mental_model_load_impl(
        &self,
        formation_id: &str,
    ) -> Result<MentalModelBundle, StoreError> {
        Ok(self
            .mental_model
            .read()
            .await
            .get(formation_id)
            .cloned()
            .unwrap_or_default())
    }

    pub(super) async fn mental_model_clear_impl(
        &self,
        formation_id: &str,
    ) -> Result<(), StoreError> {
        self.mental_model.write().await.remove(formation_id);
        Ok(())
    }
}

pub type CoopWritesMap = HashMap<String, CoopWriteEntry>;
pub type CoopDepositsMap = HashMap<String, CoopDepositEntry>;
pub type MentalModelMap = HashMap<String, MentalModelBundle>;

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
