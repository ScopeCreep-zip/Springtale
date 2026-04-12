use chrono::{DateTime, Utc};
use rusqlite::params;

use crate::error::StoreError;
use crate::schema::audit::{AuditEntry, AuditFilter};

use super::SqliteBackend;

impl SqliteBackend {
    pub(super) async fn insert_audit_entry_impl(
        &self,
        entry: &AuditEntry,
    ) -> Result<(), StoreError> {
        let conn = self.conn.clone();
        let entry = entry.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;
            conn.execute(
                "INSERT INTO audit_trail (id, timestamp, connector_name, action_type, action_summary, verdict, verdict_reason, result, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
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
                "SELECT id, timestamp, connector_name, action_type, action_summary, verdict, verdict_reason, result FROM audit_trail WHERE 1=1",
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
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;

            let mut entries = Vec::new();
            for (id_str, ts_str, connector, action_type, summary, verdict, reason, result) in rows {
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
}
