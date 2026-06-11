use chrono::{DateTime, Utc};
use rusqlite::params;

use crate::error::StoreError;
use crate::schema::audit::{AuditEntry, AuditFilter};
use crate::schema::audit_chain::compute_row_hash;

use super::SqliteBackend;

impl SqliteBackend {
    pub(super) async fn insert_audit_entry_impl(
        &self,
        entry: &AuditEntry,
    ) -> Result<(), StoreError> {
        let conn = self.conn.clone();
        let mut entry = entry.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;

            // Read the chain tip: last row's `row_hash` + `chain_seq`.
            // The first row links to the vault genesis anchor stamped
            // into row 0's `prev_hash` by the daemon at boot. If the
            // table is empty AND the boot anchor hasn't landed yet
            // (e.g. tests, in-memory ephemeral runs), the chain starts
            // from the empty string.
            //
            // SECURITY: this is the only INSERT path; the chain
            // continuity rests on this read-then-write being executed
            // under the same connection lock so no concurrent insert
            // can race a stale chain tip into the table.
            let tip: Option<(String, i64)> = conn
                .query_row(
                    "SELECT row_hash, chain_seq FROM audit_trail \
                     ORDER BY chain_seq DESC LIMIT 1",
                    [],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
                )
                .ok();
            let (prev_hash, chain_seq) = match tip {
                Some((h, s)) => (h, s + 1),
                None => {
                    // Chain is empty — the very first row chains to the
                    // vault-bound genesis anchor stored under the
                    // `audit.chain.anchor` config key by the daemon at
                    // boot. If the daemon hasn't stamped one yet (tests,
                    // pre-migration), fall back to the empty string.
                    let anchor: String = conn
                        .query_row(
                            "SELECT value_json FROM config_store WHERE key = ?1",
                            rusqlite::params!["audit.chain.anchor"],
                            |row| row.get::<_, String>(0),
                        )
                        .ok()
                        .map(|raw| serde_json::from_str::<String>(&raw).unwrap_or(raw))
                        .unwrap_or_default();
                    (anchor, 1)
                }
            };

            entry.prev_hash = prev_hash.clone();
            entry.chain_seq = chain_seq;
            entry.row_hash = compute_row_hash(&prev_hash, &entry);

            conn.execute(
                "INSERT INTO audit_trail (id, timestamp, connector_name, action_type, action_summary, verdict, verdict_reason, result, created_at, prev_hash, row_hash, chain_seq)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    entry.id.to_string(),
                    entry.timestamp.to_rfc3339(),
                    entry.connector_name,
                    entry.action_type,
                    entry.action_summary,
                    entry.verdict,
                    entry.verdict_reason,
                    entry.result,
                    Utc::now().to_rfc3339(),
                    entry.prev_hash,
                    entry.row_hash,
                    entry.chain_seq,
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }

    pub(super) async fn list_audit_entries_impl(
        &self,
        filter: &AuditFilter,
    ) -> Result<Vec<AuditEntry>, StoreError> {
        let conn = self.conn.clone();
        let filter = filter.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;

            let mut sql = String::from(
                "SELECT id, timestamp, connector_name, action_type, action_summary, verdict, verdict_reason, result, prev_hash, row_hash, chain_seq FROM audit_trail WHERE 1=1",
            );
            let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
            let mut param_idx = 1;

            if let Some(ref name) = filter.connector_name {
                sql.push_str(&format!(" AND connector_name = ?{param_idx}"));
                param_values.push(Box::new(name.clone()));
                param_idx += 1;
            }
            if let Some(ref after) = filter.after {
                sql.push_str(&format!(" AND timestamp > ?{param_idx}"));
                param_values.push(Box::new(after.to_rfc3339()));
                param_idx += 1;
            }
            if let Some(ref before) = filter.before {
                sql.push_str(&format!(" AND timestamp < ?{param_idx}"));
                param_values.push(Box::new(before.to_rfc3339()));
                param_idx += 1;
            }
            if let Some(ref verdict) = filter.verdict {
                sql.push_str(&format!(" AND verdict = ?{param_idx}"));
                param_values.push(Box::new(verdict.clone()));
                param_idx += 1;
            }

            sql.push_str(" ORDER BY timestamp DESC");

            if let Some(limit) = filter.limit {
                sql.push_str(&format!(" LIMIT ?{param_idx}"));
                param_values.push(Box::new(limit as i64));
                param_idx += 1;
            }

            if let Some(offset) = filter.offset {
                sql.push_str(&format!(" OFFSET ?{param_idx}"));
                param_values.push(Box::new(offset as i64));
                let _ = param_idx;
            }

            let params_refs: Vec<&dyn rusqlite::types::ToSql> =
                param_values.iter().map(|p| p.as_ref()).collect();

            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt
                .query_map(params_refs.as_slice(), |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                        row.get::<_, String>(9)?,
                        row.get::<_, i64>(10)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;

            let mut entries = Vec::new();
            for (
                id_str,
                ts_str,
                connector,
                action_type,
                summary,
                verdict,
                reason,
                result,
                prev_hash,
                row_hash,
                chain_seq,
            ) in rows
            {
                let id = uuid::Uuid::parse_str(&id_str)
                    .map_err(|e| StoreError::Serialization(e.to_string()))?;
                let timestamp = chrono::DateTime::parse_from_rfc3339(&ts_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .map_err(|e| StoreError::Serialization(e.to_string()))?;
                entries.push(AuditEntry {
                    id,
                    timestamp,
                    connector_name: connector,
                    action_type,
                    action_summary: summary,
                    verdict,
                    verdict_reason: reason,
                    result,
                    prev_hash,
                    row_hash,
                    chain_seq,
                });
            }
            Ok(entries)
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }

    pub(super) async fn export_audit_impl(
        &self,
        after: &DateTime<Utc>,
        before: &DateTime<Utc>,
    ) -> Result<Vec<AuditEntry>, StoreError> {
        self.list_audit_entries_impl(&AuditFilter {
            after: Some(*after),
            before: Some(*before),
            ..Default::default()
        })
        .await
    }

    pub(super) async fn delete_audit_before_impl(
        &self,
        before: &DateTime<Utc>,
    ) -> Result<u64, StoreError> {
        let conn = self.conn.clone();
        let before = *before;
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let deleted = conn.execute(
                "DELETE FROM audit_trail WHERE timestamp < ?1",
                params![before.to_rfc3339()],
            )?;
            Ok(deleted as u64)
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }

    /// Walk the audit_trail chain in `chain_seq` ascending order and
    /// recompute each row's `row_hash`. Returns the rows in walk order
    /// (suitable for the verifier to chain-check). The first row's
    /// `prev_hash` is whatever was persisted — the verifier provides
    /// the expected anchor for comparison.
    pub(super) async fn list_audit_chain_impl(&self) -> Result<Vec<AuditEntry>, StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            let mut stmt = conn.prepare(
                "SELECT id, timestamp, connector_name, action_type, action_summary, verdict, verdict_reason, result, prev_hash, row_hash, chain_seq \
                 FROM audit_trail ORDER BY chain_seq ASC",
            )?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                        row.get::<_, String>(9)?,
                        row.get::<_, i64>(10)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            let mut out = Vec::new();
            for (
                id_str,
                ts_str,
                connector,
                action_type,
                summary,
                verdict,
                reason,
                result,
                prev_hash,
                row_hash,
                chain_seq,
            ) in rows
            {
                let id = uuid::Uuid::parse_str(&id_str)
                    .map_err(|e| StoreError::Serialization(e.to_string()))?;
                let timestamp = chrono::DateTime::parse_from_rfc3339(&ts_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .map_err(|e| StoreError::Serialization(e.to_string()))?;
                out.push(AuditEntry {
                    id,
                    timestamp,
                    connector_name: connector,
                    action_type,
                    action_summary: summary,
                    verdict,
                    verdict_reason: reason,
                    result,
                    prev_hash,
                    row_hash,
                    chain_seq,
                });
            }
            Ok(out)
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }
}
