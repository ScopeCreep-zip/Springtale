use thiserror::Error;

#[derive(Debug, Error)]
pub enum RallyError {
    #[error("COOP-8001: rally exhausted: no tokens remaining")]
    Exhausted,
    #[error("COOP-8002: cascade threshold exceeded — escalating")]
    CascadeEscalating,
    #[error("COOP-8003: formation supervisor panicked")]
    SupervisorPanic,
}

impl RallyError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Exhausted => "COOP-8001",
            Self::CascadeEscalating => "COOP-8002",
            Self::SupervisorPanic => "COOP-8003",
        }
    }
}
