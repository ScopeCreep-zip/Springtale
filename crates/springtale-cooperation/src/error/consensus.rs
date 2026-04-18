use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConsensusError {
    #[error("COOP-5001: no override tokens remaining")]
    NoOverrideTokens,
    #[error("COOP-5002: consensus deadline expired")]
    Timeout,
    #[error("COOP-5003: vote not found: {0}")]
    VoteNotFound(uuid::Uuid),
}
