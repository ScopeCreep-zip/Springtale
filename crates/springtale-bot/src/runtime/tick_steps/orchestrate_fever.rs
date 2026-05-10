//! Step 14 — orchestrate at Fever tier (Patapon-style AI intent decomposition).
//!
//! When a formation reaches Fever and has an AI adapter attached, the
//! orchestrator decomposes the current intent into per-member subtasks and
//! posts them to the cooperative blackboard. Members pull from the
//! blackboard via `agent/step/scan_and_claim` (RimWorld pattern).
//!
//! Failure to orchestrate is recorded as a momentum failure so the formation
//! drops back below Fever rather than spinning forever.

use std::sync::Arc;

use tokio::sync::RwLock;

use crate::cooperation::blackboard::trait_::Blackboard;
use crate::cooperation::formation::Formation;
use crate::orchestrator::orchestrate;
use springtale_connector::registry::store::ConnectorRegistry;

pub async fn run(formation: &mut Formation, registry: &Arc<RwLock<ConnectorRegistry>>) {
    if !formation.can_orchestrate() {
        return;
    }
    match orchestrate::orchestrate_formation(formation, registry).await {
        Ok(subtasks) => {
            tracing::info!(
                formation = %formation.id.0,
                subtasks = subtasks.len(),
                "orchestrator decomposed intent into subtasks"
            );
            // Post subtasks to blackboard for members to pull (RimWorld
            // pattern). Key prefix `task:` enables `scan_tasks()` to find them.
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
