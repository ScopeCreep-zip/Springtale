//! The beat, end to end through the real pipeline (plan 1.8 / 0.3 / 1.14):
//! members act together inside one window, a slow member carries over as
//! `Requested` and lands on a later beat, and the concurrency cap holds.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::{Duration, Instant};

use tokio::sync::mpsc;

use springtale_cooperation::action::SubTask;
use springtale_cooperation::action_state::ActionState;
use springtale_cooperation::cadence::{AgentId, IntentPattern, Tick, TickReport};
use springtale_cooperation::routing::direct::assignment;
use springtale_cooperation::types::{ApprovalPolicy, FormationConstraints};

use crate::cooperation::blackboard::trait_::Blackboard;
use crate::cooperation::dispatch_outcome::REQUESTED_ALIGNMENT;
use crate::cooperation::formation::{Formation, FormationMember};

use super::super::executor::test_support::{
    CONNECTOR, Runtime, earn_fever, make_tick, runtime, wipe_task,
};

struct Beat {
    formation: Formation,
    rt: Runtime,
    tasks: Vec<SubTask>,
    reports_tx: mpsc::Sender<TickReport>,
    _reports_rx: mpsc::Receiver<TickReport>,
}

/// `members` operational members at Fever/Peak over one read-only
/// connector that sleeps `delay` per call, each with one task in its
/// direct inbox so the inbox step picks it up on the first beat.
fn beat(members: usize, delay: Duration, cap: usize) -> Beat {
    let rt = runtime(true, delay);
    let members: Vec<FormationMember> = (0..members)
        .map(|_| FormationMember::from_strings(AgentId::new(), vec![CONNECTOR.into()]))
        .collect();
    let mut formation = Formation::new_disconnected(
        members,
        IntentPattern::Execute { plan_id: None },
        FormationConstraints {
            destructive_action_policy: ApprovalPolicy::RequireConsensus,
            max_concurrent_actions: cap,
            ..FormationConstraints::default()
        },
    );
    formation.pacing = earn_fever(&mut formation);
    let tasks: Vec<SubTask> = formation
        .members
        .iter()
        .map(|m| {
            let task = wipe_task();
            assignment::assign(formation.direct_inbox.as_ref(), m.agent_id, task.clone());
            task
        })
        .collect();
    let (reports_tx, _reports_rx) = mpsc::channel(64);
    Beat {
        formation,
        rt,
        tasks,
        reports_tx,
        _reports_rx,
    }
}

async fn run_beat(b: &mut Beat, tick: &Tick) -> Vec<TickReport> {
    super::run(
        &mut b.formation,
        tick,
        &b.rt.bridge,
        &b.rt.sentinel,
        &b.rt.registry,
        &b.rt.store,
        &b.reports_tx,
        None,
    )
    .await
}

fn alignment_is(report: &TickReport, expected: f32) -> bool {
    (report.intent_alignment - expected).abs() < 1e-6
}

#[tokio::test]
async fn test_run_three_members_act_together_within_one_window() {
    let mut b = beat(3, Duration::from_millis(50), 0);
    let window = Duration::from_millis(120);
    let tick = make_tick(1, window);

    let started = Instant::now();
    let reports = run_beat(&mut b, &tick).await;
    let elapsed = started.elapsed();

    assert_eq!(reports.len(), 3, "one report per member");
    assert!(
        elapsed < window,
        "three 50 ms dispatches finished together in {elapsed:?}, not one after another"
    );
    assert!(
        reports.iter().all(|r| alignment_is(r, 1.0)),
        "every member's dispatch landed inside the beat"
    );
    assert!(
        reports
            .iter()
            .all(|r| r.latency >= Duration::from_millis(50)),
        "latency is the real connector time"
    );
    assert_eq!(b.rt.probe.executions(), 3);
    for task in &b.tasks {
        assert!(b.formation.blackboard.read_result(task.id).is_some());
    }
}

#[tokio::test]
async fn test_run_slow_member_reports_requested_then_success_after_it_resolves() {
    let mut b = beat(1, Duration::from_millis(300), 0);
    let window = Duration::from_millis(60);
    let task_id = b.tasks[0].id;

    // Beat N: the dispatch outlives the window.
    let reports = run_beat(&mut b, &make_tick(1, window)).await;
    assert_eq!(reports.len(), 1);
    assert!(
        alignment_is(&reports[0], REQUESTED_ALIGNMENT),
        "carried over, not scored as success"
    );
    let member = &b.formation.members[0];
    assert!(
        member.pending.is_some(),
        "the dispatch keeps running past the beat"
    );
    assert!(
        matches!(
            member.active_task.as_ref().map(|a| &a.state),
            Some(ActionState::Requested)
        ),
        "the member's action is Requested (0.3)"
    );
    assert_eq!(
        b.rt.probe.executions(),
        0,
        "the connector has not answered yet"
    );
    assert!(b.formation.blackboard.read_result(task_id).is_none());

    // Beat N+1, before the connector answers: still Requested.
    let reports = run_beat(&mut b, &make_tick(2, window)).await;
    assert_eq!(reports.len(), 1);
    assert!(alignment_is(&reports[0], REQUESTED_ALIGNMENT));

    // First beat after it resolves: Success, result posted, slot cleared.
    tokio::time::sleep(Duration::from_millis(400)).await;
    let reports = run_beat(&mut b, &make_tick(3, window)).await;
    assert_eq!(reports.len(), 1);
    assert!(
        alignment_is(&reports[0], 1.0),
        "the finished dispatch is this beat's result"
    );
    assert!(reports[0].latency >= Duration::from_millis(300));
    let member = &b.formation.members[0];
    assert!(!member.pending.is_some());
    assert!(member.active_task.is_none());
    assert_eq!(b.rt.probe.executions(), 1);
    assert!(b.formation.blackboard.read_result(task_id).is_some());
}

#[tokio::test]
async fn test_run_max_concurrent_actions_one_never_overlaps_dispatches() {
    let mut b = beat(3, Duration::from_millis(30), 1);
    let reports = run_beat(&mut b, &make_tick(1, Duration::from_secs(2))).await;

    assert_eq!(reports.len(), 3);
    assert!(
        reports.iter().all(|r| alignment_is(r, 1.0)),
        "the queue drained inside the window"
    );
    assert_eq!(b.rt.probe.executions(), 3);
    assert_eq!(
        b.rt.probe.max_in_flight(),
        1,
        "no two dispatches overlapped under cap 1"
    );
}
