//! SQLite backend for cooperation primitives — CAS writes, environment
//! deposits, and shared mental model. All operations run inside
//! `tokio::task::spawn_blocking` per the workspace rusqlite pattern.

use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{OptionalExtension, params};

use crate::error::StoreError;
use crate::schema::cooperation::{CoopCasOutcome, CoopDepositRow};
use crate::schema::mental_model::{
    MentalModelBundle, MentalModelCapabilityRow, MentalModelConventionRow, MentalModelDomainRow,
    MentalModelPatternRow, MentalModelVocabularyRow,
};

use super::SqliteBackend;

impl SqliteBackend {
    pub(super) async fn coop_cas_write_impl(
        &self,
        tick: i64,
        writer: String,
        key: String,
        expected: Option<Vec<u8>>,
        proposed: Vec<u8>,
    ) -> Result<CoopCasOutcome, StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let tx = guard
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(|e| StoreError::Database(format!("begin: {e}")))?;

            let current: Option<(Vec<u8>, String, i64)> = tx
                .query_row(
                    "SELECT value, writer, tick FROM coop_writes WHERE k = ?1",
                    params![key],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()
                .map_err(|e| StoreError::Database(format!("read: {e}")))?;

            let matches = match (&expected, &current) {
                (None, None) => true,
                (Some(e), Some((c, _, _))) => e == c,
                _ => false,
            };

            if !matches {
                // Mismatch — surface the conflicting writer's state.
                let outcome = match current {
                    Some((v, w, t)) => CoopCasOutcome::Mismatch {
                        current_value: v,
                        current_writer: w,
                        current_tick: t,
                    },
                    None => CoopCasOutcome::Mismatch {
                        current_value: Vec::new(),
                        current_writer: String::new(),
                        current_tick: 0,
                    },
                };
                // No write on mismatch — rollback implicit on drop.
                return Ok(outcome);
            }

            tx.execute(
                "INSERT OR REPLACE INTO coop_writes(k, value, writer, tick)
                 VALUES(?1, ?2, ?3, ?4)",
                params![key, proposed, writer, tick],
            )
            .map_err(|e| StoreError::Database(format!("write: {e}")))?;
            tx.commit()
                .map_err(|e| StoreError::Database(format!("commit: {e}")))?;
            Ok(CoopCasOutcome::Applied)
        })
        .await
        .map_err(|e| StoreError::Database(format!("join: {e}")))?
    }

    pub(super) async fn coop_deposit_impl(
        &self,
        location: String,
        payload: Vec<u8>,
        depositor: String,
        ttl_secs: Option<i64>,
    ) -> Result<(), StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let now = unix_now();
            let expires = ttl_secs.map(|t| now + t);
            let guard = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            guard
                .execute(
                    "INSERT OR REPLACE INTO coop_deposits
                     (location, payload, depositor, deposited_at, expires_at, claimed_by)
                     VALUES(?1, ?2, ?3, ?4, ?5, NULL)",
                    params![location, payload, depositor, now, expires],
                )
                .map_err(|e| StoreError::Database(format!("deposit: {e}")))?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(format!("join: {e}")))?
    }

    pub(super) async fn coop_collect_impl(
        &self,
        location: String,
        collector: String,
    ) -> Result<Option<Vec<u8>>, StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let tx = guard
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(|e| StoreError::Database(format!("begin: {e}")))?;
            let payload: Option<Vec<u8>> = tx
                .query_row(
                    "UPDATE coop_deposits SET claimed_by = ?1
                     WHERE location = ?2 AND claimed_by IS NULL
                     RETURNING payload",
                    params![collector, location],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| StoreError::Database(format!("claim: {e}")))?;
            if payload.is_some() {
                // Delete the claimed deposit — same semantics as sled::remove.
                tx.execute(
                    "DELETE FROM coop_deposits WHERE location = ?1 AND claimed_by = ?2",
                    params![location, collector],
                )
                .map_err(|e| StoreError::Database(format!("cleanup: {e}")))?;
            }
            tx.commit()
                .map_err(|e| StoreError::Database(format!("commit: {e}")))?;
            Ok(payload)
        })
        .await
        .map_err(|e| StoreError::Database(format!("join: {e}")))?
    }

    pub(super) async fn coop_sweep_expired_impl(&self) -> Result<u64, StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let now = unix_now();
            let guard = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let n = guard
                .execute(
                    "DELETE FROM coop_deposits
                     WHERE expires_at IS NOT NULL AND expires_at < ?1",
                    params![now],
                )
                .map_err(|e| StoreError::Database(format!("sweep: {e}")))?;
            Ok(n as u64)
        })
        .await
        .map_err(|e| StoreError::Database(format!("join: {e}")))?
    }

    pub(super) async fn coop_list_deposits_impl(&self) -> Result<Vec<CoopDepositRow>, StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let guard = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let mut stmt = guard
                .prepare(
                    "SELECT location, payload, depositor, deposited_at,
                            expires_at, claimed_by
                     FROM coop_deposits",
                )
                .map_err(|e| StoreError::Database(format!("prep list: {e}")))?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(CoopDepositRow {
                        location: r.get(0)?,
                        payload: r.get(1)?,
                        depositor: r.get(2)?,
                        deposited_at: r.get(3)?,
                        expires_at: r.get(4)?,
                        claimed_by: r.get(5)?,
                    })
                })
                .map_err(|e| StoreError::Database(format!("query list: {e}")))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| StoreError::Database(format!("row list: {e}")))?;
            Ok(rows)
        })
        .await
        .map_err(|e| StoreError::Database(format!("join: {e}")))?
    }

    pub(super) async fn mental_model_save_impl(
        &self,
        formation_id: String,
        bundle: MentalModelBundle,
    ) -> Result<(), StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let tx = guard
                .transaction()
                .map_err(|e| StoreError::Database(format!("begin: {e}")))?;

            for (table, _label) in [
                ("mental_model_domain", "domain"),
                ("mental_model_capability", "capability"),
                ("mental_model_pattern", "pattern"),
                ("mental_model_vocabulary", "vocabulary"),
                ("mental_model_convention", "convention"),
            ] {
                tx.execute(
                    &format!("DELETE FROM {table} WHERE formation_id = ?1"),
                    params![formation_id],
                )
                .map_err(|e| StoreError::Database(format!("clear {table}: {e}")))?;
            }

            for r in &bundle.domain {
                tx.execute(
                    "INSERT INTO mental_model_domain
                     (formation_id, key, description, learned_at_unix, confidence)
                     VALUES(?1, ?2, ?3, ?4, ?5)",
                    params![
                        formation_id,
                        r.key,
                        r.description,
                        r.learned_at_unix,
                        r.confidence
                    ],
                )
                .map_err(|e| StoreError::Database(format!("insert domain: {e}")))?;
            }
            for r in &bundle.capability {
                tx.execute(
                    "INSERT INTO mental_model_capability
                     (formation_id, agent_id, capability) VALUES(?1, ?2, ?3)",
                    params![formation_id, r.agent_id, r.capability],
                )
                .map_err(|e| StoreError::Database(format!("insert cap: {e}")))?;
            }
            for r in &bundle.pattern {
                tx.execute(
                    "INSERT INTO mental_model_pattern
                     (formation_id, trigger_text, participants_json,
                      success_count, failure_count, last_used_unix)
                     VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        formation_id,
                        r.trigger_text,
                        r.participants_json,
                        r.success_count,
                        r.failure_count,
                        r.last_used_unix
                    ],
                )
                .map_err(|e| StoreError::Database(format!("insert pattern: {e}")))?;
            }
            for r in &bundle.vocabulary {
                tx.execute(
                    "INSERT INTO mental_model_vocabulary
                     (formation_id, term, meaning, established_by_json)
                     VALUES(?1, ?2, ?3, ?4)",
                    params![formation_id, r.term, r.meaning, r.established_by_json],
                )
                .map_err(|e| StoreError::Database(format!("insert vocab: {e}")))?;
            }
            for r in &bundle.convention {
                tx.execute(
                    "INSERT INTO mental_model_convention
                     (formation_id, description, established_by_json, strength)
                     VALUES(?1, ?2, ?3, ?4)",
                    params![
                        formation_id,
                        r.description,
                        r.established_by_json,
                        r.strength
                    ],
                )
                .map_err(|e| StoreError::Database(format!("insert conv: {e}")))?;
            }
            tx.commit()
                .map_err(|e| StoreError::Database(format!("commit: {e}")))?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(format!("join: {e}")))?
    }

    pub(super) async fn mental_model_load_impl(
        &self,
        formation_id: String,
    ) -> Result<MentalModelBundle, StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let guard = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let mut bundle = MentalModelBundle::default();

            let mut stmt = guard
                .prepare(
                    "SELECT key, description, learned_at_unix, confidence
                     FROM mental_model_domain WHERE formation_id = ?1",
                )
                .map_err(|e| StoreError::Database(format!("prep domain: {e}")))?;
            bundle.domain = stmt
                .query_map(params![formation_id], |r| {
                    Ok(MentalModelDomainRow {
                        key: r.get(0)?,
                        description: r.get(1)?,
                        learned_at_unix: r.get(2)?,
                        confidence: r.get(3)?,
                    })
                })
                .map_err(|e| StoreError::Database(format!("query domain: {e}")))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| StoreError::Database(format!("row domain: {e}")))?;
            drop(stmt);

            let mut stmt = guard
                .prepare(
                    "SELECT agent_id, capability FROM mental_model_capability
                     WHERE formation_id = ?1",
                )
                .map_err(|e| StoreError::Database(format!("prep cap: {e}")))?;
            bundle.capability = stmt
                .query_map(params![formation_id], |r| {
                    Ok(MentalModelCapabilityRow {
                        agent_id: r.get(0)?,
                        capability: r.get(1)?,
                    })
                })
                .map_err(|e| StoreError::Database(format!("query cap: {e}")))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| StoreError::Database(format!("row cap: {e}")))?;
            drop(stmt);

            let mut stmt = guard
                .prepare(
                    "SELECT trigger_text, participants_json, success_count,
                            failure_count, last_used_unix
                     FROM mental_model_pattern WHERE formation_id = ?1",
                )
                .map_err(|e| StoreError::Database(format!("prep pattern: {e}")))?;
            bundle.pattern = stmt
                .query_map(params![formation_id], |r| {
                    Ok(MentalModelPatternRow {
                        trigger_text: r.get(0)?,
                        participants_json: r.get(1)?,
                        success_count: r.get(2)?,
                        failure_count: r.get(3)?,
                        last_used_unix: r.get(4)?,
                    })
                })
                .map_err(|e| StoreError::Database(format!("query pattern: {e}")))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| StoreError::Database(format!("row pattern: {e}")))?;
            drop(stmt);

            let mut stmt = guard
                .prepare(
                    "SELECT term, meaning, established_by_json
                     FROM mental_model_vocabulary WHERE formation_id = ?1",
                )
                .map_err(|e| StoreError::Database(format!("prep vocab: {e}")))?;
            bundle.vocabulary = stmt
                .query_map(params![formation_id], |r| {
                    Ok(MentalModelVocabularyRow {
                        term: r.get(0)?,
                        meaning: r.get(1)?,
                        established_by_json: r.get(2)?,
                    })
                })
                .map_err(|e| StoreError::Database(format!("query vocab: {e}")))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| StoreError::Database(format!("row vocab: {e}")))?;
            drop(stmt);

            let mut stmt = guard
                .prepare(
                    "SELECT description, established_by_json, strength
                     FROM mental_model_convention WHERE formation_id = ?1",
                )
                .map_err(|e| StoreError::Database(format!("prep conv: {e}")))?;
            bundle.convention = stmt
                .query_map(params![formation_id], |r| {
                    Ok(MentalModelConventionRow {
                        description: r.get(0)?,
                        established_by_json: r.get(1)?,
                        strength: r.get(2)?,
                    })
                })
                .map_err(|e| StoreError::Database(format!("query conv: {e}")))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| StoreError::Database(format!("row conv: {e}")))?;
            drop(stmt);

            Ok(bundle)
        })
        .await
        .map_err(|e| StoreError::Database(format!("join: {e}")))?
    }

    pub(super) async fn mental_model_clear_impl(
        &self,
        formation_id: String,
    ) -> Result<(), StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let tx = guard
                .transaction()
                .map_err(|e| StoreError::Database(format!("begin: {e}")))?;
            for table in [
                "mental_model_domain",
                "mental_model_capability",
                "mental_model_pattern",
                "mental_model_vocabulary",
                "mental_model_convention",
            ] {
                tx.execute(
                    &format!("DELETE FROM {table} WHERE formation_id = ?1"),
                    params![formation_id],
                )
                .map_err(|e| StoreError::Database(format!("clear {table}: {e}")))?;
            }
            tx.commit()
                .map_err(|e| StoreError::Database(format!("commit: {e}")))?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(format!("join: {e}")))?
    }
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
