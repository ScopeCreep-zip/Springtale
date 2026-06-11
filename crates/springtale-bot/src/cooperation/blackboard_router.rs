//! `BlackboardRouter` — bridges the concrete `CooperativeBlackboard` and
//! the formation's L3 substrates (DirectInbox + FlexibleChainPool) to the
//! `TaskRouter` trait so the agent loop can consume work through one
//! canonical interface.
//!
//! Routing layers handled by this impl:
//! - **L3 direct (`COOPERATION.md §20.1`)** — `poll_assigned` first checks
//!   the per-agent `DirectInbox` (populated by `dispatch_handoff` Direct
//!   variants), then falls back to scanning the blackboard for tasks with
//!   `assigned_to == agent` (B3 CBBA replan path).
//! - **L3 work-stealing (`§20.4`)** — `try_steal_chain` consults
//!   `FlexibleChainPool::find_task` per capability and translates the
//!   stolen `HandoffPayload` into a synthetic `SubTask` so downstream
//!   claim/dispatch sees a uniform shape.
//! - **L1 routine (`§20.2`)** — `scan` queries the blackboard with the
//!   capability filter, tier-gated by `LayerAuthority::allows(tier, L1Routine)`.

use std::sync::Arc;

use async_trait::async_trait;

use springtale_cooperation::action::{SubTask, SubTaskResult};
use springtale_cooperation::authority;
use springtale_cooperation::awareness::LocalAwareness;
use springtale_cooperation::cadence::AgentId;
use springtale_cooperation::capability::CapabilityDecl;
use springtale_cooperation::handoff::{FlexibleChainPool, HandoffPayload};
use springtale_cooperation::layer::LayerId;
use springtale_cooperation::momentum::MomentumTier;
use springtale_cooperation::routing::direct::DirectInbox;
use springtale_cooperation::routing::trait_::TaskRouter;
use springtale_cooperation::routing::types::{PriorityTask, RoutingError, TaskClaim, TaskId};

use crate::cooperation::blackboard::cooperative::CooperativeBlackboard;
use crate::cooperation::blackboard::trait_::Blackboard;
use crate::orchestrator::fuel::FuelBudget;

/// Concrete `TaskRouter` backed by a `CooperativeBlackboard` plus the
/// formation's L3 substrates.
///
/// All four references are `Arc` so the router can be cloned cheaply into
/// per-member runner tasks. `direct_inbox` and `flex_chain_pool` are also
/// `Arc`-shared with `Formation` itself (the same instances are addressed
/// from `Formation::dispatch_handoff` writes and from this router's reads).
pub struct BlackboardRouter {
    pub blackboard: Arc<CooperativeBlackboard>,
    pub fuel: Arc<FuelBudget>,
    pub direct_inbox: Arc<DirectInbox>,
    pub flex_chain_pool: Arc<FlexibleChainPool>,
}

impl BlackboardRouter {
    pub fn new(
        blackboard: Arc<CooperativeBlackboard>,
        fuel: Arc<FuelBudget>,
        direct_inbox: Arc<DirectInbox>,
        flex_chain_pool: Arc<FlexibleChainPool>,
    ) -> Self {
        Self {
            blackboard,
            fuel,
            direct_inbox,
            flex_chain_pool,
        }
    }
}

#[async_trait]
impl TaskRouter for BlackboardRouter {
    async fn poll_assigned(&self, agent: AgentId) -> Option<PriorityTask> {
        // Direct handoff inbox takes precedence — `dispatch_handoff` Direct
        // variants push the SubTask directly here, so the agent receives
        // explicitly-targeted work without a blackboard round-trip.
        if let Some(task) = self.direct_inbox.poll(agent) {
            return Some(PriorityTask::new(task));
        }
        // Fallback: blackboard tasks tagged `assigned_to == agent` (B3
        // CBBA replan path). Lowest priority value wins.
        self.blackboard
            .scan_tasks(&[])
            .into_iter()
            .filter(|t| t.assigned_to == Some(agent))
            .min_by_key(|t| t.priority)
            .map(PriorityTask::new)
    }

    async fn try_steal_chain(
        &self,
        capabilities: &[CapabilityDecl],
        agent: AgentId,
    ) -> Option<PriorityTask> {
        // Iterate the agent's capabilities and steal from the first pool
        // that yields work. Each `find_task` consults local worker deque
        // → global injector → peer stealers (crossbeam Chase-Lev pattern).
        for cap in capabilities {
            if let Some(payload) = self.flex_chain_pool.find_task(cap, agent) {
                return Some(PriorityTask::new(subtask_from_chain_payload(
                    cap.clone(),
                    agent,
                    &payload,
                )));
            }
        }
        None
    }

    async fn scan(
        &self,
        capabilities: &[CapabilityDecl],
        tier: MomentumTier,
        awareness: Option<&LocalAwareness>,
    ) -> Option<PriorityTask> {
        if !authority::allows(tier, LayerId::L1Routine) {
            return None;
        }
        let candidates = self.blackboard.scan_tasks(capabilities);
        if candidates.is_empty() {
            return None;
        }
        // B5: at Warming+ tier, weight priority by neighbor recent-success
        // — peer TickReports for the same target connector contribute a
        // negative bias (lower effective priority = more urgent under our
        // 1-is-best convention). Cold tier ignores awareness and uses the
        // raw blackboard priority.
        if tier >= MomentumTier::Warming
            && let Some(aw) = awareness
        {
            return candidates
                .into_iter()
                .min_by(|a, b| {
                    weighted_priority(a, aw)
                        .partial_cmp(&weighted_priority(b, aw))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(PriorityTask::new);
        }
        // Lowest raw priority value wins (1 is more urgent than 5). The
        // existing PriorityTask Ord inverts so BinaryHeap pops smallest;
        // here we just use min_by_key for simplicity.
        candidates
            .into_iter()
            .min_by_key(|t| t.priority)
            .map(PriorityTask::new)
    }

    async fn claim(&self, task_id: TaskId, agent: AgentId) -> Result<TaskClaim, RoutingError> {
        match self
            .blackboard
            .claim_task(&task_id.to_string(), agent, &self.fuel)
        {
            Ok(()) => Ok(TaskClaim {
                task_id,
                owner: agent,
                claimed_at: std::time::Instant::now(),
            }),
            Err(_) => Err(RoutingError::LostRace(task_id)),
        }
    }

    async fn complete(&self, _task_id: TaskId, result: SubTaskResult) {
        let _ = self.blackboard.post_result(&result, &self.fuel);
    }

    async fn release(&self, task_id: TaskId) {
        self.blackboard.release_task(&task_id.to_string());
    }
}

/// B5 priority weighting — derive an effective priority that lowers
/// (= more urgent) when neighbors recently succeeded on the candidate's
/// target connector. Returned as `f32` so the bias can be sub-integer.
///
/// Bias model:
/// - Each peer TickReport whose `action_taken.target` matches the
///   candidate's `target_connector.name` AND whose `intent_alignment >= 0.7`
///   contributes a -0.2 bias (cap at -1.0 total).
/// - Negative interference reports for the candidate's connector
///   contribute +0.3 (de-prioritize work peers struggle with).
fn weighted_priority(task: &SubTask, awareness: &LocalAwareness) -> f32 {
    let connector = task.target_connector.name.as_str();
    let mut bias: f32 = 0.0;
    for report in &awareness.last_tick_reports {
        let Some(desc) = report.action_taken.as_ref() else {
            continue;
        };
        let Some(target) = desc.target.as_deref() else {
            continue;
        };
        if target != connector {
            continue;
        }
        if report.intent_alignment >= 0.7 {
            bias -= 0.2;
        }
        if !report.interference_with.is_empty() {
            bias += 0.3;
        }
    }
    let bias = bias.clamp(-1.0, 1.0);
    task.priority as f32 + bias
}

/// Translate a stolen `HandoffPayload` into a synthetic `SubTask` so the
/// per-agent executor sees a uniform shape across L1 scans, L3 direct
/// dequeues, and L3 chain steals. Priority 3 matches the chain-step
/// priority used by `dispatch_handoff_durable::subtask_for_chain` — chain
/// steps are mid-priority (more urgent than scans, less than direct).
fn subtask_from_chain_payload(
    capability: CapabilityDecl,
    receiver: AgentId,
    payload: &HandoffPayload,
) -> SubTask {
    SubTask {
        id: uuid::Uuid::new_v4(),
        target_connector: capability.clone(),
        action_name: "chain_step".to_owned(),
        params: payload.data.clone(),
        priority: 3,
        assigned_to: Some(receiver),
        description: format!("flex-chain step on capability {}", capability.name),
        depends_on: Vec::new(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use springtale_cooperation::cadence::ActionDescriptor;
    use uuid::Uuid;

    fn make_task(connector: &str, priority: u8) -> SubTask {
        SubTask {
            id: Uuid::new_v4(),
            target_connector: CapabilityDecl::new(connector),
            action_name: "send".into(),
            params: serde_json::json!({}),
            priority,
            assigned_to: None,
            description: "task".into(),
            depends_on: Vec::new(),
        }
    }

    fn router_with(tasks: Vec<SubTask>) -> BlackboardRouter {
        let blackboard = Arc::new(CooperativeBlackboard::new());
        let fuel = Arc::new(FuelBudget::new(1_000_000));
        let trace = Uuid::new_v4();
        for task in &tasks {
            blackboard
                .write(
                    &format!("task:{}", task.id),
                    serde_json::to_value(task).unwrap(),
                    trace,
                    &fuel,
                )
                .expect("write");
        }
        BlackboardRouter::new(
            blackboard,
            fuel,
            Arc::new(DirectInbox::new()),
            Arc::new(FlexibleChainPool::new()),
        )
    }

    /// Per `COOPERATION.md §7` capability table, L1 routine scans are
    /// allowed at every tier including Cold — so this proves the Cold-tier
    /// path is unchanged from the bare `blackboard.scan_tasks` call.
    /// Plan §B5: "No behaviour change for Cold tier".
    #[tokio::test]
    async fn scan_works_at_cold_tier_no_behavior_change() {
        let router = router_with(vec![make_task("slack", 1)]);
        let caps = vec![CapabilityDecl::new("slack")];
        let pick = router.scan(&caps, MomentumTier::Cold, None).await;
        assert!(pick.is_some(), "L1 routine scan must work at Cold tier");
    }

    #[tokio::test]
    async fn scan_picks_lowest_priority_value_at_warming() {
        let urgent = make_task("slack", 1);
        let urgent_id = urgent.id;
        let router = router_with(vec![make_task("slack", 5), urgent]);
        let caps = vec![CapabilityDecl::new("slack")];
        let pick = router
            .scan(&caps, MomentumTier::Warming, None)
            .await
            .expect("some");
        assert_eq!(pick.id(), urgent_id);
    }

    #[tokio::test]
    async fn scan_filters_by_capability() {
        let router = router_with(vec![make_task("slack", 1), make_task("github", 1)]);
        let caps = vec![CapabilityDecl::new("github")];
        let pick = router
            .scan(&caps, MomentumTier::Warming, None)
            .await
            .expect("some");
        assert_eq!(pick.task.target_connector.name, "github");
    }

    /// B5 plan: at Warming+, peer TickReports with high alignment for a
    /// connector lower the effective priority of tasks targeting that
    /// connector. Two tasks with equal raw priority — peer success on
    /// "github" tilts the pick toward github.
    #[tokio::test]
    async fn scan_warming_prefers_peer_success_target() {
        use springtale_cooperation::cadence::{ActionDescriptor, TickReport};

        let slack = make_task("slack", 5);
        let github = make_task("github", 5);
        let github_id = github.id;
        let router = router_with(vec![slack, github]);
        let caps = vec![CapabilityDecl::new("slack"), CapabilityDecl::new("github")];

        let mut aw = LocalAwareness::default();
        aw.record_tick_reports(vec![TickReport {
            agent_id: AgentId::new(),
            tick_sequence: springtale_cooperation::TickId(1),
            action_taken: Some(ActionDescriptor {
                kind: "task_claimed".into(),
                target: Some("github".into()),
                payload_hash: 0,
            }),
            latency: std::time::Duration::from_millis(0),
            intent_alignment: 0.95,
            interference_with: vec![],
        }]);

        let pick = router
            .scan(&caps, MomentumTier::Warming, Some(&aw))
            .await
            .expect("some");
        assert_eq!(
            pick.id(),
            github_id,
            "warming tier with peer success on github should prefer github"
        );
    }

    #[tokio::test]
    async fn poll_assigned_returns_only_directed_tasks() {
        let agent = AgentId::new();
        let other = AgentId::new();
        let mut directed = make_task("slack", 1);
        directed.assigned_to = Some(agent);
        let directed_id = directed.id;
        let mut foreign = make_task("slack", 1);
        foreign.assigned_to = Some(other);
        let router = router_with(vec![directed, foreign, make_task("slack", 1)]);

        let pick = router.poll_assigned(agent).await.expect("some");
        assert_eq!(pick.id(), directed_id);
    }

    /// B6: direct inbox takes precedence over the blackboard's `assigned_to`
    /// scan. A SubTask pushed to the inbox is returned immediately on
    /// `poll_assigned`, ahead of any blackboard task that also targets
    /// the agent.
    #[tokio::test]
    async fn poll_assigned_prefers_direct_inbox() {
        let agent = AgentId::new();
        let mut directed = make_task("slack", 1);
        directed.assigned_to = Some(agent);
        let blackboard_id = directed.id;
        let router = router_with(vec![directed]);

        let inbox_task = make_task("github", 5);
        let inbox_id = inbox_task.id;
        router.direct_inbox.push(agent, inbox_task);

        let pick = router.poll_assigned(agent).await.expect("some");
        assert_eq!(
            pick.id(),
            inbox_id,
            "direct inbox wins over assigned_to scan"
        );
        assert_ne!(pick.id(), blackboard_id);
    }

    /// B6: try_steal_chain finds payloads in the FlexibleChainPool for
    /// capabilities the agent holds. The stolen HandoffPayload is wrapped
    /// in a synthetic SubTask so downstream sees a uniform shape.
    #[tokio::test]
    async fn try_steal_chain_returns_payload_for_capability() {
        let router = router_with(vec![]);
        let agent = AgentId::new();
        let cap = CapabilityDecl::new("github");
        router.flex_chain_pool.register(cap.clone(), agent);
        router.flex_chain_pool.post(
            &cap,
            HandoffPayload {
                data: serde_json::json!({"step": 1}),
                schema: "github_chain".into(),
                produced_by: ActionDescriptor {
                    kind: "produce".into(),
                    target: None,
                    payload_hash: 0,
                },
                consumable_by: vec![cap.clone()],
                expires: None,
            },
        );

        let pick = router
            .try_steal_chain(std::slice::from_ref(&cap), agent)
            .await
            .expect("some");
        assert_eq!(pick.task.target_connector, cap);
        assert_eq!(pick.task.action_name, "chain_step");
        assert_eq!(pick.task.assigned_to, Some(agent));
    }

    #[tokio::test]
    async fn try_steal_chain_returns_none_when_pool_empty() {
        let router = router_with(vec![]);
        let agent = AgentId::new();
        let cap = CapabilityDecl::new("github");
        router.flex_chain_pool.register(cap.clone(), agent);
        // No post — pool is empty for this capability.
        assert!(
            router.try_steal_chain(&[cap], agent).await.is_none(),
            "empty pool yields None"
        );
    }
}
