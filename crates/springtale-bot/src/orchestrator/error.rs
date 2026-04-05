use thiserror::Error;

/// Orchestrator-specific errors.
#[derive(Debug, Error)]
pub enum OrchestratorError {
    #[error("maximum pipeline depth {depth} exceeds limit {max}")]
    MaxDepthExceeded { depth: u32, max: u32 },

    #[error("too many children: {count} exceeds limit {max}")]
    MaxConcurrentExceeded { count: usize, max: usize },

    #[error("fuel exhausted: requested {requested}, remaining {remaining}")]
    FuelExhausted { requested: u64, remaining: u64 },

    #[error("child pipeline failed: {0}")]
    ChildFailed(String),

    #[error("pipeline error: {0}")]
    Pipeline(#[from] springtale_core::pipeline::error::PipelineError),
}
