use async_trait::async_trait;

use super::context::PipelineContext;
use super::error::PipelineError;

/// A single stage in a pipeline.
///
/// Stages are composable: each receives a context, transforms it, and
/// returns the updated context. Stages are stateless — all state lives
/// in the `PipelineContext`.
#[async_trait]
pub trait Stage: Send + Sync {
    /// Human-readable name for logging and error reporting.
    fn name(&self) -> &str;

    /// Execute this stage, transforming the pipeline context.
    async fn call(&self, ctx: PipelineContext) -> Result<PipelineContext, PipelineError>;
}
