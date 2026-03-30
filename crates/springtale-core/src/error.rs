use thiserror::Error;

/// Top-level error type for springtale-core.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("pipeline error at stage {stage}: {source}")]
    Pipeline {
        stage: usize,
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("rule parse error: {0}")]
    RuleParse(String),

    #[error("condition evaluation error: {0}")]
    ConditionEval(String),

    #[error("template resolution error: {0}")]
    Template(String),
}
