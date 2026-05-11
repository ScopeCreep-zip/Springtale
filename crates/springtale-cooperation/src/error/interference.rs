use thiserror::Error;

#[derive(Debug, Error)]
pub enum InterferenceError {
    #[error("COOP-7001: interference detected: {0}")]
    Detected(String),
}

impl InterferenceError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Detected(_) => "COOP-7001",
        }
    }
}
