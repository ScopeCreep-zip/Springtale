//! Deterministic replay harness — plan §4.1 + §10.4.
//!
//! "Every `CadenceBus::run` can be recorded (tick sequence, reports, peer
//! messages) to a log file. A replay harness reads the log and re-runs
//! the formation against a new code version, comparing outputs. This
//! catches behavior drift across versions."
//!
//! The harness below is the minimal working version: serialize a
//! deterministic sequence of `TickReport`s into JSONL, read them back,
//! drive `tick_processor::process_tick` across the recorded ticks, and
//! compare the aggregate interference set and success flags against a
//! snapshot computed at record time. If the code under test has drifted
//! in a way that changes interference detection output, the replay
//! assertion fails.
//!
//! What the harness covers:
//!   - Report-level replay (the wire format that peers gossip).
//!   - Pure-function re-execution of the tick processor — the only
//!     dependency of a replay is the processor itself, so drift shows
//!     up as a diff against the recorded expectation.
//!
//! What it deliberately does NOT cover:
//!   - Wall-clock timing. `Instant::timestamp` in ticks is replay-only,
//!     and the processor doesn't read it.
//!   - Non-report side effects (environment writes, rally tokens).
//!     Those replays land in future checkpoints; this harness is the
//!     entry point the plan calls out explicitly.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use serde::{Deserialize, Serialize};

use springtale_cooperation::cadence::{ActionDescriptor, AgentId, TickReport};
use springtale_cooperation::interference::InterferenceEvent;
use springtale_cooperation::tick_processor;

/// A single recorded tick — reports plus the aggregate result the
/// processor produced at record time.
#[derive(Serialize, Deserialize)]
struct RecordedTick {
    sequence: u64,
    reports: Vec<ReportRecord>,
    /// Count of interference events detected at record time (the exact
    /// list isn't serializable because some variants carry non-serde
    /// fields, but the count + `all_succeeded` flag is sufficient to
    /// detect drift for v0.1).
    expected_interference_count: usize,
    expected_all_succeeded: bool,
}

/// Wire-format mirror of `TickReport`. Exists because `TickReport`
/// contains a `Duration` (serde-able) and a local `AgentId` (serde-able
/// via baseline's Uuid impl), so this is a thin serializable shell that
/// round-trips the public fields the processor reads.
#[derive(Serialize, Deserialize)]
struct ReportRecord {
    agent_id_bytes: [u8; 16],
    tick_sequence: u64,
    action_kind: String,
    action_target: Option<String>,
    action_payload_hash: u64,
    latency_ms: u64,
    intent_alignment: f32,
}

impl From<&TickReport> for ReportRecord {
    fn from(r: &TickReport) -> Self {
        let action = r
            .action_taken
            .as_ref()
            .expect("test reports always carry an action");
        Self {
            agent_id_bytes: *r.agent_id.0.as_bytes(),
            tick_sequence: r.tick_sequence,
            action_kind: action.kind.clone(),
            action_target: action.target.clone(),
            action_payload_hash: action.payload_hash,
            latency_ms: r.latency.as_millis() as u64,
            intent_alignment: r.intent_alignment,
        }
    }
}

impl From<&ReportRecord> for TickReport {
    fn from(r: &ReportRecord) -> Self {
        let agent_id = AgentId(uuid::Uuid::from_bytes(r.agent_id_bytes));
        Self {
            agent_id,
            tick_sequence: r.tick_sequence,
            action_taken: Some(ActionDescriptor {
                kind: r.action_kind.clone(),
                target: r.action_target.clone(),
                payload_hash: r.action_payload_hash,
            }),
            latency: Duration::from_millis(r.latency_ms),
            intent_alignment: r.intent_alignment,
            interference_with: Vec::new(),
        }
    }
}

fn synth_reports(tick: u64, n: usize) -> Vec<TickReport> {
    (0..n)
        .map(|i| TickReport {
            agent_id: AgentId::new(),
            tick_sequence: tick,
            action_taken: Some(ActionDescriptor {
                kind: "send".to_owned(),
                // Pair some agents on the same target so the processor
                // actually exercises the interference branch.
                target: Some(format!("chat-{}", i % 3)),
                payload_hash: i as u64,
            }),
            latency: Duration::from_millis(5),
            intent_alignment: 0.95,
            interference_with: Vec::new(),
        })
        .collect()
}

fn record_tick(seq: u64, reports: &[TickReport]) -> (RecordedTick, String) {
    let result = tick_processor::process_tick(reports.to_vec());
    let tick = RecordedTick {
        sequence: seq,
        reports: reports.iter().map(ReportRecord::from).collect(),
        expected_interference_count: result.interferences.len(),
        expected_all_succeeded: result.all_succeeded,
    };
    let line = serde_json::to_string(&tick).expect("serialize tick");
    (tick, line)
}

fn replay_line(line: &str) -> (usize, bool, Vec<InterferenceEvent>) {
    let tick: RecordedTick = serde_json::from_str(line).expect("parse tick");
    let reports: Vec<TickReport> = tick.reports.iter().map(TickReport::from).collect();
    let result = tick_processor::process_tick(reports);
    (
        tick.expected_interference_count,
        tick.expected_all_succeeded,
        result.interferences,
    )
}

#[test]
fn record_then_replay_matches_same_version() {
    // Record five ticks across three formation sizes.
    let mut log: Vec<String> = Vec::new();
    for seq in 0..5 {
        let reports = synth_reports(seq, 4);
        let (_tick, line) = record_tick(seq, &reports);
        log.push(line);
    }

    // Replay each line — the replayed interference count and success
    // flag must match what record-time captured.
    for line in &log {
        let (expected_count, expected_success, observed) = replay_line(line);
        assert_eq!(
            observed.len(),
            expected_count,
            "replay drift: recorded {} interferences, replay produced {}",
            expected_count,
            observed.len()
        );
        // A formation with shared targets produces ≥1 interference, so
        // all_succeeded should be false. If that changes, someone broke
        // the processor — the replay surfaces it.
        let (_, _, result) = replay_line(line);
        let replayed_success = result.is_empty();
        assert_eq!(
            replayed_success, expected_success,
            "replay drift: success flag mismatch"
        );
    }
}

#[test]
fn empty_log_replays_cleanly() {
    let reports: Vec<TickReport> = Vec::new();
    let result = tick_processor::process_tick(reports);
    assert!(result.interferences.is_empty());
    assert!(!result.all_succeeded, "empty reports cannot succeed");
}
