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
