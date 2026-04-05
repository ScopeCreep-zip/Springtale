use std::sync::Arc;

use serde::Deserialize;
use tokio::sync::Semaphore;
use uuid::Uuid;

use springtale_core::pipeline::context::PipelineContext;

use super::error::OrchestratorError;
use super::fuel::FuelBudget;

/// Configuration for the recursive orchestrator.
#[derive(Debug, Clone, Deserialize)]
pub struct OrchestratorConfig {
    /// Maximum concurrent children per spawn point. Default: 8.
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: usize,
    /// Maximum recursive depth. Default: 4.
    #[serde(default = "default_max_depth")]
    pub max_depth: u32,
    /// Default fuel budget for root pipelines. Default: 10,000,000.
    #[serde(default = "default_fuel")]
    pub default_fuel: u64,
}

fn default_max_concurrent() -> usize {
    8
}
fn default_max_depth() -> u32 {
    4
}
fn default_fuel() -> u64 {
    10_000_000
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            max_concurrent: default_max_concurrent(),
            max_depth: default_max_depth(),
            default_fuel: default_fuel(),
        }
    }
}

/// A child pipeline task to be spawned.
pub struct ChildTask {
    /// Read-only snapshot of parent context (input for this child).
    pub context: PipelineContext,
    /// What this child should compute (as a function returning output).
    /// In Phase 2a, children are simple: they receive context and return output.
    pub label: String,
}

/// Result from a completed child pipeline.
#[derive(Debug)]
pub struct ChildResult {
    /// Trace ID of the child pipeline.
    pub trace_id: Uuid,
    /// Label of the child task.
    pub label: String,
    /// Output from the child (or error message).
    pub output: Result<serde_json::Value, String>,
    /// Fuel consumed by this child.
    pub fuel_used: u64,
}

/// The recursive pipeline orchestrator.
///
/// Spawns child pipelines with fuel budgets, enforcing depth and
/// concurrency limits. Children inherit read-only context snapshots
/// and cannot escalate capabilities.
pub struct Orchestrator {
    config: OrchestratorConfig,
}

impl Orchestrator {
    pub fn new(config: OrchestratorConfig) -> Self {
        Self { config }
    }

    /// Spawn child pipelines from a parent context.
    ///
    /// - Children inherit a read-only clone of parent context (snapshot)
    /// - Fuel: `parent_remaining / num_children` per child
    /// - Concurrency bounded by `config.max_concurrent` via Semaphore
    /// - Depth bounded by `config.max_depth`
    /// - Results collected via JoinSet
    pub async fn spawn_children(
        &self,
        parent_ctx: &PipelineContext,
        parent_fuel: &FuelBudget,
        tasks: Vec<ChildTask>,
    ) -> Result<Vec<ChildResult>, OrchestratorError> {
        // Validate depth
        let current_depth = parent_ctx.chain_depth;
        if current_depth >= self.config.max_depth {
            return Err(OrchestratorError::MaxDepthExceeded {
                depth: current_depth + 1,
                max: self.config.max_depth,
            });
        }

        // Validate concurrency
        if tasks.len() > self.config.max_concurrent {
            return Err(OrchestratorError::MaxConcurrentExceeded {
                count: tasks.len(),
                max: self.config.max_concurrent,
            });
        }

        if tasks.is_empty() {
            return Ok(vec![]);
        }

        // Split fuel among children
        let child_fuels = parent_fuel.split(tasks.len() as u32)?;

        // Semaphore for concurrency bounding
        let semaphore = Arc::new(Semaphore::new(self.config.max_concurrent));

        // Spawn children
        let mut join_set = tokio::task::JoinSet::new();

        for (task, fuel) in tasks.into_iter().zip(child_fuels.into_iter()) {
            let sem = semaphore.clone();
            let child_ctx = parent_ctx.child()?;
            let initial_fuel = fuel.initial();
            let label = task.label.clone();
            let trace_id = child_ctx.trace_id;

            join_set.spawn(async move {
                let _permit = sem.acquire().await;

                // Child executes with its context and fuel
                // Phase 2a: children are simple context transforms
                // Phase 3: children run full compose_pipeline()
                let output = child_ctx.output.clone();
                let fuel_used = initial_fuel - fuel.remaining();

                ChildResult {
                    trace_id,
                    label,
                    output: Ok(output),
                    fuel_used,
                }
            });
        }

        // Collect results
        let mut results = Vec::new();
        while let Some(join_result) = join_set.join_next().await {
            match join_result {
                Ok(child_result) => results.push(child_result),
                Err(e) => {
                    tracing::error!(error = %e, "child pipeline panicked");
                    results.push(ChildResult {
                        trace_id: Uuid::new_v4(),
                        label: "panicked".into(),
                        output: Err(format!("child panicked: {e}")),
                        fuel_used: 0,
                    });
                }
            }
        }

        Ok(results)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn test_ctx() -> PipelineContext {
        PipelineContext::new(serde_json::json!({"test": true}))
    }

    #[tokio::test]
    async fn test_spawn_children_basic() {
        let orch = Orchestrator::new(OrchestratorConfig::default());
        let fuel = FuelBudget::new(1000);
        let tasks = vec![
            ChildTask {
                context: test_ctx(),
                label: "child1".into(),
            },
            ChildTask {
                context: test_ctx(),
                label: "child2".into(),
            },
        ];

        let results = orch
            .spawn_children(&test_ctx(), &fuel, tasks)
            .await
            .unwrap();
        assert_eq!(results.len(), 2);
        for r in &results {
            assert!(r.output.is_ok());
        }
    }

    #[tokio::test]
    async fn test_depth_exceeded() {
        let orch = Orchestrator::new(OrchestratorConfig {
            max_depth: 2,
            ..Default::default()
        });
        let fuel = FuelBudget::new(1000);

        // Create context at depth 2 (at the limit)
        let mut ctx = test_ctx();
        ctx.chain_depth = 2;

        let tasks = vec![ChildTask {
            context: test_ctx(),
            label: "deep".into(),
        }];

        let result = orch.spawn_children(&ctx, &fuel, tasks).await;
        assert!(matches!(
            result,
            Err(OrchestratorError::MaxDepthExceeded { .. })
        ));
    }

    #[tokio::test]
    async fn test_concurrent_exceeded() {
        let orch = Orchestrator::new(OrchestratorConfig {
            max_concurrent: 2,
            ..Default::default()
        });
        let fuel = FuelBudget::new(1000);

        let tasks = vec![
            ChildTask {
                context: test_ctx(),
                label: "a".into(),
            },
            ChildTask {
                context: test_ctx(),
                label: "b".into(),
            },
            ChildTask {
                context: test_ctx(),
                label: "c".into(),
            },
        ];

        let result = orch.spawn_children(&test_ctx(), &fuel, tasks).await;
        assert!(matches!(
            result,
            Err(OrchestratorError::MaxConcurrentExceeded { .. })
        ));
    }

    #[tokio::test]
    async fn test_empty_tasks() {
        let orch = Orchestrator::new(OrchestratorConfig::default());
        let fuel = FuelBudget::new(1000);
        let results = orch
            .spawn_children(&test_ctx(), &fuel, vec![])
            .await
            .unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_fuel_split() {
        let orch = Orchestrator::new(OrchestratorConfig::default());
        let fuel = FuelBudget::new(1000);
        let tasks = vec![
            ChildTask {
                context: test_ctx(),
                label: "a".into(),
            },
            ChildTask {
                context: test_ctx(),
                label: "b".into(),
            },
        ];

        let _results = orch
            .spawn_children(&test_ctx(), &fuel, tasks)
            .await
            .unwrap();
        // Parent fuel consumed: 500 * 2 = 1000
        assert_eq!(fuel.remaining(), 0);
    }
}
