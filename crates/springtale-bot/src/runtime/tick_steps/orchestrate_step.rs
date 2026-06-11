//! Orchestrate step — decompose the formation's intent into per-member subtasks.
//!
//! Two paths, selected inside [`orchestrate::orchestrate_formation`]:
//!
//! - **AI augmentation** (Patapon Fever mechanic): when an AI adapter is
//!   attached *and* the formation has earned Fever momentum, an LLM decomposes
//!   the intent into rich, parameterised subtasks.
//! - **Deterministic default**: otherwise, subtasks are derived mechanically
//!   from member connector capabilities + the `IntentPattern`. This is what
//!   gives a `NoopAdapter` formation outward effect, and it runs across tiers
//!   (gated by the momentum × layer authority matrix), not only at Fever.
//!
//! Either way the subtasks land on the cooperative blackboard under the
//! `task:*` prefix; members pull them via `agent/step/scan_and_claim` (RimWorld
//! pattern) and execute through `dispatch_action` (sentinel + autonomy gating).
//!
//! Failure to orchestrate via the AI path is recorded as a momentum failure so
//! the formation drops back below Fever rather than spinning forever.

use std::sync::Arc;

use tokio::sync::RwLock;

use crate::cooperation::blackboard::trait_::Blackboard;
use crate::cooperation::formation::Formation;
use crate::orchestrator::orchestrate;
use springtale_connector::registry::store::ConnectorRegistry;

pub async fn run(formation: &mut Formation, registry: &Arc<RwLock<ConnectorRegistry>>) {
    // No operational members → nothing to orchestrate.
    if !formation.is_viable() {
        return;
    }
    match orchestrate::orchestrate_formation(formation, registry).await {
        Ok(subtasks) => {
            if subtasks.is_empty() {
                return;
            }
            tracing::info!(
                formation = %formation.id.0,
                subtasks = subtasks.len(),
                "orchestrator decomposed intent into subtasks"
            );
            // Post subtasks to blackboard for members to pull (RimWorld
            // pattern). Key prefix `task:` enables `scan_tasks()` to find them.
            // Deterministic subtasks carry stable ids, so re-posting the same
            // poll each tick overwrites rather than accumulating.
            let trace_id = uuid::Uuid::new_v4();
            for task in &subtasks {
                let task_key = format!("task:{}", task.id);
                if let Err(e) = formation.blackboard.write(
                    &task_key,
                    serde_json::to_value(task).unwrap_or_default(),
                    trace_id,
                    &formation.fuel,
                ) {
                    tracing::warn!(task = %task.id, error = %e, "failed to post subtask to blackboard");
                }
            }
        }
        Err(e) => {
            tracing::warn!(formation = %formation.id.0, error = %e, "orchestration failed");
            formation.momentum.record_failure();
        }
    }
}
