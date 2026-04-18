use thiserror::Error;

#[derive(Debug, Error)]
pub enum PacingError {
    #[error("COOP-B001: pacing violation: {0}")]
    Violation(String),
}
