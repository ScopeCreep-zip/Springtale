use chrono::Utc;
use rusqlite::params;

use crate::error::StoreError;
use crate::schema::jobs::{JobId, JobRow};

use super::SqliteBackend;

impl SqliteBackend {
    pub(super) async fn enqueue_job_impl(&self, job: &JobRow) -> Result<JobId, StoreError> {
        let conn = self.conn.lock().await;
        let payload_str = serde_json::to_string(&job.payload)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        conn.execute(
            "INSERT INTO jobs (id, payload, status, attempts, max_attempts, created_at, started_at, last_error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                job.id.0.to_string(),
                payload_str,
                job.status,
                job.attempts as i64,
                job.max_attempts as i64,
                job.created_at.to_rfc3339(),
                job.started_at.as_ref().map(|t| t.to_rfc3339()),
                job.last_error,
            ],
        )?;
        Ok(job.id)
    }

    pub(super) async fn dequeue_job_impl(&self) -> Result<Option<JobRow>, StoreError> {
        let conn = self.conn.lock().await;

        // Find the oldest pending job and mark it as running in one step
        let now = Utc::now().to_rfc3339();

        // First find the job
        let maybe_id: Option<String> = conn
            .query_row(
                "SELECT id FROM jobs WHERE status = 'pending' ORDER BY created_at ASC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .ok();

        let Some(job_id) = maybe_id else {
            return Ok(None);
        };

        // Mark it as running
        conn.execute(
            "UPDATE jobs SET status = 'running', started_at = ?1, attempts = attempts + 1 WHERE id = ?2",
            params![now, job_id],
        )?;

        // Read the full job back
        let row = conn.query_row(
            "SELECT id, payload, status, attempts, max_attempts, created_at, started_at, last_error FROM jobs WHERE id = ?1",
            params![job_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                ))
            },
        )?;

        let id =
            uuid::Uuid::parse_str(&row.0).map_err(|e| StoreError::Serialization(e.to_string()))?;
        let payload: serde_json::Value =
            serde_json::from_str(&row.1).map_err(|e| StoreError::Serialization(e.to_string()))?;
        let created_at = chrono::DateTime::parse_from_rfc3339(&row.5)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        let started_at = row
            .6
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc));

        Ok(Some(JobRow {
            id: JobId(id),
            payload,
            status: row.2,
            attempts: row.3 as u32,
            max_attempts: row.4 as u32,
            created_at,
            started_at,
            last_error: row.7,
        }))
    }

    pub(super) async fn complete_job_impl(&self, id: &JobId) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        let updated = conn.execute(
            "UPDATE jobs SET status = 'complete' WHERE id = ?1",
            params![id.0.to_string()],
        )?;
        if updated == 0 {
            return Err(StoreError::NotFound {
                entity: "job".into(),
                id: id.to_string(),
            });
        }
        Ok(())
    }

    pub(super) async fn fail_job_impl(&self, id: &JobId, error: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        let updated = conn.execute(
            "UPDATE jobs SET status = 'failed', last_error = ?1 WHERE id = ?2",
            params![error, id.0.to_string()],
        )?;
        if updated == 0 {
            return Err(StoreError::NotFound {
                entity: "job".into(),
                id: id.to_string(),
            });
        }
        Ok(())
    }
}
