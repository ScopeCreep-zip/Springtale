use springtale_core::pipeline::context::PipelineContext;

use super::error::OrchestratorError;
use super::recursive::ChildTask;

/// Create a child task from a parent context with a scoped fuel budget.
///
/// The child receives a read-only snapshot of the parent's output as
/// its input. It cannot access the parent's capabilities or modify
/// the parent's state.
pub fn spawn_child_task(
    parent_ctx: &PipelineContext,
    label: &str,
) -> Result<ChildTask, OrchestratorError> {
    let child_ctx = parent_ctx.child()?;

    Ok(ChildTask {
        context: child_ctx,
        label: label.to_owned(),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_spawn_child_task() {
        let parent = PipelineContext::new(serde_json::json!({"data": "test"}));
        let task = spawn_child_task(&parent, "test-child").unwrap();
        assert_eq!(task.label, "test-child");
    }

    #[test]
    fn test_child_inherits_parent_output() {
        let mut parent = PipelineContext::new(serde_json::json!({}));
        parent.output = serde_json::json!({"result": 42});
        let task = spawn_child_task(&parent, "child").unwrap();
        // Child's input is parent's output
        assert_eq!(task.context.input, serde_json::json!({"result": 42}));
    }

    #[test]
    fn test_child_depth_incremented() {
        let parent = PipelineContext::new(serde_json::json!({}));
        let task = spawn_child_task(&parent, "child").unwrap();
        assert_eq!(task.context.chain_depth, 1);
    }
}
