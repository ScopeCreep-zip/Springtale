//! Act-phase half of the executor: the connector call, owning every
//! handle it needs so it can be spawned and outlive the beat.

use std::sync::Arc;
use std::time::Instant;

use springtale_cooperation::AutonomyLevel;
use springtale_cooperation::MomentumTier;
use springtale_cooperation::action::SubTask;
use springtale_cooperation::action_state::ActionState;
use springtale_cooperation::cadence::{ActionDescriptor, AgentId};
use springtale_cooperation::types::ApprovalPolicy;

use crate::cooperation::blackboard::cooperative::CooperativeBlackboard;

use super::{Dispatched, ExecuteOutcome};

/// A claimed task cleared to run, with `Arc` clones of everything the
/// dispatch touches. No `&mut Formation` — `post` does the write-back.
pub struct DispatchJob {
    pub agent: AgentId,
    pub formation_id: uuid::Uuid,
    pub formation_momentum: MomentumTier,
    pub destructive_policy: ApprovalPolicy,
    pub autonomy: AutonomyLevel,
    pub task: SubTask,
    pub descriptor: ActionDescriptor,
    pub bridge: springtale_runtime::CapabilityBridge,
    pub sentinel: Arc<springtale_sentinel::Sentinel>,
    pub blackboard: Arc<CooperativeBlackboard>,
}

pub async fn dispatch_one(job: DispatchJob) -> ExecuteOutcome {
    // W3 cross-agent data pipe: materialize upstream results into this
    // task's params (`${result:<uuid>...}`) before building the action.
    // The scan-side dependency gate guarantees the results exist.
    let mut task = job.task;
    crate::cooperation::task_dispatch::resolve_result_params(&mut task, job.blackboard.as_ref());
    let action = crate::cooperation::task_dispatch::subtask_to_action(&task);
    let exec_start = Instant::now();

    // Formation-beat path: the cooperation envelope carries the firing
    // agent + formation + momentum tier so the dispatcher's per-tier WASM
    // `InstancePre` selection matches the call site (§16). The synthetic
    // RuleId is fine — formation-task dispatch is rule-less; the
    // executions log keys off `bot_id` + `formation_id` instead.
    let execution = springtale_cooperation::execution::ExecutionContext::for_formation(
        springtale_core::rule::RuleId::new(),
        job.agent,
        springtale_cooperation::types::FormationId(job.formation_id),
        job.formation_momentum,
        springtale_cooperation::execution::ExecutionMode::Cooperation,
        job.destructive_policy,
        job.autonomy,
    );
    let exec_result = springtale_runtime::dispatch::dispatch_action(
        &action,
        &job.bridge,
        &job.sentinel,
        execution,
        serde_json::Value::Null,
    )
    .await;
    let duration_ms = exec_start.elapsed().as_millis() as u64;

    let (success, output) = match &exec_result {
        Ok(chain) => (
            true,
            chain
                .steps
                .last()
                .map(|s| s.output.clone())
                .unwrap_or_else(|| serde_json::json!({ "result": chain.brief() })),
        ),
        Err(err) => (false, serde_json::json!({"error": err.to_string()})),
    };
    // The sentinel's `Throttle` sleeps inside `dispatch_action` and then
    // proceeds; the chain counts it. A throttled step that then failed
    // is reported as the failure.
    let throttled = exec_result.as_ref().is_ok_and(|chain| chain.throttles > 0);
    let error = exec_result.err().map(|e| e.to_string());
    let denied = sentinel_denied(error.as_deref());
    let state = if success {
        ActionState::Success
    } else {
        ActionState::Failure(error.clone().unwrap_or_default())
    };

    ExecuteOutcome {
        action_descriptor: Some(job.descriptor),
        alignment: if success { 1.0 } else { 0.3 },
        consensus_task: None,
        state,
        duration_ms,
        dispatched: Some(Dispatched {
            task,
            action,
            success,
            output,
            error,
        }),
        throttled,
        denied,
    }
}

/// The sentinel's `Quarantine` / `Pause` verdicts reach the executor only
/// as a failed chain step whose message `dispatch_action` prefixes with
/// the verdict name.
fn sentinel_denied(error: Option<&str>) -> bool {
    error.is_some_and(|e| e.contains("sentinel quarantined:") || e.contains("sentinel paused:"))
}
