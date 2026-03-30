use thiserror::Error;

#[derive(Debug, Error)]
pub enum SchedulerError {
    #[error("invalid cron expression: {0}")]
    InvalidCron(String),

    #[error("filesystem watcher error: {0}")]
    Watcher(String),

    #[error("job execution failed: {0}")]
    JobFailed(String),

    #[error("job queue error: {0}")]
    QueueError(String),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("max retry attempts ({attempts}) exceeded")]
    MaxRetriesExceeded { attempts: u32 },
}
