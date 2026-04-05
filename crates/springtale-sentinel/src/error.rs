use thiserror::Error;

#[derive(Debug, Error)]
pub enum SentinelError {
    #[error("rate limit exceeded for connector: {0}")]
    RateLimitExceeded(String),

    #[error("circuit breaker open for stage: {0}")]
    CircuitOpen(String),

    #[error("dead-man switch triggered: {0}")]
    DeadManTriggered(String),

    #[error("toxic capability pair detected: {0}")]
    ToxicPair(String),

    #[error("destructive action requires approval: {0}")]
    ApprovalRequired(String),

    #[error("storage error: {0}")]
    Storage(#[from] springtale_store::StoreError),
}
