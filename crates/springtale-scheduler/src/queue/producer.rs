use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::SchedulerError;

/// A job in the queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    /// Unique job identifier.
    pub id: Uuid,
    /// The action payload (JSON-serialized rule action).
    pub payload: serde_json::Value,
    /// Job status.
    pub status: JobStatus,
    /// Number of execution attempts.
    pub attempts: u32,
    /// Maximum retry attempts.
    pub max_attempts: u32,
    /// When the job was created (ISO 8601).
    pub created_at: String,
    /// When the job was last attempted (ISO 8601).
    pub started_at: Option<String>,
    /// Error message from the last failed attempt.
    pub last_error: Option<String>,
}

/// Job status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Pending,
    Running,
    Complete,
    Failed,
}

/// Produces (enqueues) jobs into the queue.
///
/// Phase 1a: in-memory via tokio mpsc channel. When springtale-store (M7)
/// ships with `StorageBackend::enqueue_job()`, the application layer will
/// persist jobs to SQLite for durability across restarts. The producer API
/// stays the same — only the backing changes.
pub struct JobProducer {
    tx: tokio::sync::mpsc::Sender<Job>,
}

impl JobProducer {
    /// Create a new producer with the given channel.
    pub fn new(tx: tokio::sync::mpsc::Sender<Job>) -> Self {
        Self { tx }
    }

    /// Enqueue a new job.
    pub async fn enqueue(
        &self,
        payload: serde_json::Value,
        max_attempts: u32,
    ) -> Result<Uuid, SchedulerError> {
        let id = Uuid::new_v4();
        let job = Job {
            id,
            payload,
            status: JobStatus::Pending,
            attempts: 0,
            max_attempts,
            created_at: chrono::Utc::now().to_rfc3339(),
            started_at: None,
            last_error: None,
        };

        self.tx
            .send(job)
            .await
            .map_err(|_| SchedulerError::QueueError("job channel closed".into()))?;

        tracing::debug!(job_id = %id, "job enqueued");
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_enqueue_job() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        let producer = JobProducer::new(tx);

        let id = producer
            .enqueue(serde_json::json!({"action": "test"}), 3)
            .await
            .unwrap();

        let job = rx.recv().await.unwrap();
        assert_eq!(job.id, id);
        assert_eq!(job.status, JobStatus::Pending);
        assert_eq!(job.attempts, 0);
        assert_eq!(job.max_attempts, 3);
    }
}
