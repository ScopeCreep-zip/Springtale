use chrono::{DateTime, Utc};
use rusqlite::params;

use crate::error::StoreError;
use crate::schema::events::{EventEntry, EventFilter};

use super::SqliteBackend;

impl SqliteBackend {
    pub(super) async fn log_event_impl(&self, event: &EventEntry) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO events (id, connector_name, trigger_type, timestamp, action_taken)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                event.id.to_string(),
                event.connector_name,
                event.trigger_type,
                event.timestamp.to_rfc3339(),
                event.action_taken,
            ],
        )?;
        Ok(())
    }

    pub(super) async fn list_events_impl(
        &self,
        filter: &EventFilter,
    ) -> Result<Vec<EventEntry>, StoreError> {
        let conn = self.conn.lock().await;

        let mut sql = String::from(
            "SELECT id, connector_name, trigger_type, timestamp, action_taken FROM events WHERE 1=1",
        );
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut param_idx = 1;

        if let Some(ref name) = filter.connector_name {
            sql.push_str(&format!(" AND connector_name = ?{param_idx}"));
            param_values.push(Box::new(name.clone()));
            param_idx += 1;
        }
        if let Some(ref tt) = filter.trigger_type {
            sql.push_str(&format!(" AND trigger_type = ?{param_idx}"));
            param_values.push(Box::new(tt.clone()));
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

        sql.push_str(" ORDER BY timestamp DESC");

        if let Some(limit) = filter.limit {
            sql.push_str(&format!(" LIMIT ?{param_idx}"));
            param_values.push(Box::new(limit as i64));
            param_idx += 1;
        }

        if let Some(offset) = filter.offset {
            sql.push_str(&format!(" OFFSET ?{param_idx}"));
            param_values.push(Box::new(offset as i64));
            let _ = param_idx; // suppress unused warning
        }

        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params_refs.as_slice(), |row| {
                let id_str: String = row.get(0)?;
                let ts_str: String = row.get(3)?;
                Ok((id_str, row.get(1)?, row.get(2)?, ts_str, row.get(4)?))
            })?
            .collect::<Result<Vec<(String, String, String, String, String)>, _>>()?;

        let mut events = Vec::new();
        for (id_str, connector_name, trigger_type, ts_str, action_taken) in rows {
            let id = uuid::Uuid::parse_str(&id_str)
                .map_err(|e| StoreError::Serialization(e.to_string()))?;
            let timestamp = chrono::DateTime::parse_from_rfc3339(&ts_str)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| StoreError::Serialization(e.to_string()))?;
            events.push(EventEntry {
                id,
                connector_name,
                trigger_type,
                timestamp,
                action_taken,
            });
        }
        Ok(events)
    }

    pub(super) async fn delete_events_before_impl(
        &self,
        before: &DateTime<Utc>,
    ) -> Result<u64, StoreError> {
        let conn = self.conn.lock().await;
        let deleted = conn.execute(
            "DELETE FROM events WHERE timestamp < ?1",
            params![before.to_rfc3339()],
        )?;
        Ok(deleted as u64)
    }
}
