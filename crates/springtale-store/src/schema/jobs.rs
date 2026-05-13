use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

/// Unique identifier for a job in the queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct JobId(pub Uuid);

impl JobId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for JobId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for JobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Row type for the `jobs` table (queue).
///
/// The store owns this type. The application layer maps between `JobRow`
/// and the scheduler's `Job` type.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct JobRow {
    /// Unique job identifier.
    pub id: JobId,
    /// Action payload (JSON).
    pub payload: serde_json::Value,
    /// Job status: "pending", "running", "complete", "failed".
    pub status: String,
    /// Number of execution attempts.
    pub attempts: u32,
    /// Maximum retry attempts.
    pub max_attempts: u32,
    /// When the job was created.
    pub created_at: DateTime<Utc>,
    /// When the job was last started.
    pub started_at: Option<DateTime<Utc>>,
    /// Error message from the last failed attempt.
    pub last_error: Option<String>,
}
