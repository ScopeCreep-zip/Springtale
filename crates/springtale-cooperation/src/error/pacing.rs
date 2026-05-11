use thiserror::Error;

#[derive(Debug, Error)]
pub enum PacingError {
    #[error("COOP-B001: pacing violation: {0}")]
    Violation(String),
}

impl PacingError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Violation(_) => "COOP-B001",
        }
    }
}
