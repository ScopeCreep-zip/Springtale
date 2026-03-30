use thiserror::Error;

/// Error type for pipeline execution failures.
#[derive(Debug, Error)]
pub enum PipelineError {
    /// A stage failed during execution.
    #[error("pipeline stage {stage} failed: {source}")]
    StageFailed {
        stage: usize,
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// The pipeline exceeded its fuel budget.
    #[error("pipeline fuel exhausted at stage {stage} (used {used}, limit {limit})")]
    FuelExhausted { stage: usize, used: u64, limit: u64 },

    /// Chain depth exceeded the maximum allowed.
    #[error("chain depth {depth} exceeds maximum {max}")]
    ChainDepthExceeded { depth: u32, max: u32 },
}
