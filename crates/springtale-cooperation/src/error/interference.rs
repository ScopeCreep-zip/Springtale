use thiserror::Error;

#[derive(Debug, Error)]
pub enum InterferenceError {
    #[error("COOP-7001: interference detected: {0}")]
    Detected(String),
}
