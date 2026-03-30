use super::context::PipelineContext;
use super::error::PipelineError;
use super::stage::Stage;

/// Compose a sequence of stages into a pipeline.
///
/// Stages execute left-to-right. Each stage receives the context output
/// by the previous stage. If any stage fails, execution short-circuits
/// with a `PipelineError` containing the stage index.
///
/// ```text
/// Stage 0 → Stage 1 → Stage 2 → ... → Final Context
///              ↓ (failure)
///         PipelineError { stage: 1, source: ... }
/// ```
pub async fn compose_pipeline(
    stages: &[Box<dyn Stage>],
    mut ctx: PipelineContext,
) -> Result<PipelineContext, PipelineError> {
    for (index, stage) in stages.iter().enumerate() {
        let trace_id = ctx.trace_id;
        let stage_name = stage.name().to_owned();

        tracing::debug!(
            trace_id = %trace_id,
            stage = index,
            name = %stage_name,
            "executing pipeline stage"
        );

        ctx = stage.call(ctx).await.map_err(|e| {
            tracing::warn!(
                trace_id = %trace_id,
                stage = index,
                name = %stage_name,
                error = %e,
                "pipeline stage failed"
            );
            match e {
                PipelineError::StageFailed { .. } => e,
                other => PipelineError::StageFailed {
                    stage: index,
                    source: Box::new(other),
                },
            }
        })?;
    }
    Ok(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct AddFieldStage {
        key: String,
        value: serde_json::Value,
    }

    #[async_trait]
    impl Stage for AddFieldStage {
        fn name(&self) -> &str {
            "add_field"
        }

        async fn call(&self, mut ctx: PipelineContext) -> Result<PipelineContext, PipelineError> {
            if let serde_json::Value::Object(ref mut map) = ctx.output {
                map.insert(self.key.clone(), self.value.clone());
            }
            Ok(ctx)
        }
    }

    struct FailStage;

    #[async_trait]
    impl Stage for FailStage {
        fn name(&self) -> &str {
            "fail"
        }

        async fn call(&self, _ctx: PipelineContext) -> Result<PipelineContext, PipelineError> {
            Err(PipelineError::StageFailed {
                stage: 0,
                source: "intentional failure".into(),
            })
        }
    }

    #[tokio::test]
    async fn test_compose_empty_pipeline() {
        let ctx = PipelineContext::new(serde_json::json!({}));
        let stages: Vec<Box<dyn Stage>> = vec![];
        let result = compose_pipeline(&stages, ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_compose_single_stage() {
        let ctx = PipelineContext::new(serde_json::json!({}));
        let stages: Vec<Box<dyn Stage>> = vec![Box::new(AddFieldStage {
            key: "added".into(),
            value: serde_json::json!(true),
        })];
        let result = compose_pipeline(&stages, ctx).await;
        let ctx = result.as_ref().ok();
        assert!(ctx.is_some());
        assert_eq!(
            ctx.map(|c| c.output.get("added").cloned()),
            Some(Some(serde_json::json!(true)))
        );
    }

    #[tokio::test]
    async fn test_compose_chain_stages() {
        let ctx = PipelineContext::new(serde_json::json!({}));
        let stages: Vec<Box<dyn Stage>> = vec![
            Box::new(AddFieldStage {
                key: "a".into(),
                value: serde_json::json!(1),
            }),
            Box::new(AddFieldStage {
                key: "b".into(),
                value: serde_json::json!(2),
            }),
        ];
        let result = compose_pipeline(&stages, ctx).await;
        let out = &result.as_ref().ok().map(|c| &c.output);
        assert!(out.is_some());
        let out = out.cloned();
        assert_eq!(
            out.as_ref().and_then(|v| v.get("a").cloned()),
            Some(serde_json::json!(1))
        );
        assert_eq!(
            out.as_ref().and_then(|v| v.get("b").cloned()),
            Some(serde_json::json!(2))
        );
    }

    #[tokio::test]
    async fn test_compose_failure_short_circuits() {
        let ctx = PipelineContext::new(serde_json::json!({}));
        let stages: Vec<Box<dyn Stage>> = vec![
            Box::new(AddFieldStage {
                key: "a".into(),
                value: serde_json::json!(1),
            }),
            Box::new(FailStage),
            Box::new(AddFieldStage {
                key: "should_not_exist".into(),
                value: serde_json::json!(true),
            }),
        ];
        let result = compose_pipeline(&stages, ctx).await;
        assert!(result.is_err());
    }
}
