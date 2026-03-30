#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod cron;
pub mod error;
pub mod queue;
pub mod retry;
pub mod watcher;

pub use cron::CronExecutor;
pub use error::SchedulerError;
pub use queue::consumer::JobConsumer;
pub use queue::producer::{Job, JobProducer, JobStatus};
pub use retry::backoff::{BackoffConfig, RetryState};
pub use watcher::FsWatcher;
