//! Interference detection — O(N²) pairwise write-set analysis.
//!
//! Per COOPERATION.md §13.3: compare ActionRecords to detect conflicts.
//! Per event sourcing: optimistic concurrency — two writes to same key = conflict.
//! Per LangGraph: reducer conflict = two agents update same state field.
//! Per OT (Operational Transform): same-key same-value = idempotent (redundancy),
//! same-key different-value = conflict.

use crate::cadence::TickReport;
use crate::state::EnvironmentWrite;

use super::{ActionRecord, InterferenceEvent, InterferenceType};

/// Detect interference from ActionRecords using write-set analysis.
///
/// Per COOPERATION.md §13.3: O(N²) pairwise comparison.
/// For each pair of agents:
/// - Write-set intersection with different values → ResourceConflict (0.8)
/// - Write-set intersection with same values → Redundancy (0.2)
/// - Side-effect key in other's read-set → CollateralDamage (side_effect.magnitude)
pub fn detect_from_records(tick: u64, records: &[ActionRecord]) -> Vec<InterferenceEvent> {
    let mut events = Vec::new();

    for (i, a) in records.iter().enumerate() {
        for b in records.iter().skip(i + 1) {
            for (key, val_a) in &a.write_set {
                if let Some(val_b) = b.write_set.get(key) {
                    let redundant = val_a == val_b;
                    events.push(InterferenceEvent {
                        tick_sequence: tick,
                        agent_a: a.agent,
                        agent_b: b.agent,
                        interference_type: if redundant {
                            InterferenceType::Redundancy
                        } else {
                            InterferenceType::ResourceConflict
                        },
                        severity: if redundant { 0.2 } else { 0.8 },
                    });
                }
            }

            for se in &a.side_effects {
                if b.read_set.contains(&se.affected_key) {
                    events.push(InterferenceEvent {
                        tick_sequence: tick,
                        agent_a: a.agent,
                        agent_b: b.agent,
                        interference_type: InterferenceType::CollateralDamage,
                        severity: se.magnitude,
                    });
                }
            }

            for se in &b.side_effects {
                if a.read_set.contains(&se.affected_key) {
                    events.push(InterferenceEvent {
                        tick_sequence: tick,
                        agent_a: b.agent,
                        agent_b: a.agent,
                        interference_type: InterferenceType::CollateralDamage,
                        severity: se.magnitude,
                    });
                }
            }
        }
    }

    events
}

/// Detect interference including cross-tick ActionNegation.
///
/// Per COOPERATION.md §13.1/§13.4: a current-tick write that undoes a
/// prior-tick write by a different agent is `ActionNegation`. This cannot
/// be detected from the current tick's records alone — it requires the
/// ordered write-log history (the same `write_log` carried in
/// [`WorkspaceSnapshot`]).
///
/// Rules: for each current-tick write `(A, K, V)`:
/// 1. Find the most recent prior write `(B, K, V_b)` in `history` where
///    `B != A`. If none, skip — no one to negate.
/// 2. If `V` is `Null` and `V_b` is not `Null`, this is a clear-negation.
/// 3. Otherwise, if `V` equals any value for `K` recorded before
///    `B`'s write (i.e. a pre-B historical value), this is a
///    revert-negation.
/// 4. In either case, emit `InterferenceType::ActionNegation`.
///
/// Returns all events detected by [`detect_from_records`] plus any
/// ActionNegation events from the history pass. Duplicates are possible
/// when the same pair also appears in a same-tick ResourceConflict; the
/// caller deduplicates if needed.
///
/// [`WorkspaceSnapshot`]: crate::state::WorkspaceSnapshot
pub fn detect_from_records_with_history(
    tick: u64,
    records: &[ActionRecord],
    history: &[EnvironmentWrite],
) -> Vec<InterferenceEvent> {
    let mut events = detect_from_records(tick, records);

    for rec in records {
        for (key, new_val) in &rec.write_set {
            // Find most recent prior writer of this key who is NOT rec.agent.
            let prior_b = history
                .iter()
                .rev()
                .filter(|w| w.key == *key)
                .find(|w| w.writer != rec.agent);
            let Some(b_write) = prior_b else {
                continue;
            };

            // Pre-B historical values for this key: every write to K with
            // timestamp strictly earlier than B's write.
            let pre_b_values: Vec<&serde_json::Value> = history
                .iter()
                .take_while(|w| w.timestamp < b_write.timestamp)
                .filter(|w| w.key == *key)
                .map(|w| &w.value)
                .collect();

            let is_null_clear = new_val.is_null() && !b_write.value.is_null();
            let reverts_pre_b = pre_b_values.contains(&new_val);
            let distinct_from_b = new_val != &b_write.value;

            if (is_null_clear || reverts_pre_b) && distinct_from_b {
                events.push(InterferenceEvent {
                    tick_sequence: tick,
                    agent_a: rec.agent,
                    agent_b: b_write.writer,
                    interference_type: InterferenceType::ActionNegation,
                    severity: 0.7,
                });
            }
        }
    }

    events
}

/// Detect interference from TickReports using pre-flagged interference_with.
///
/// Kept for the current tick pipeline until M14 migrates the event loop
/// to produce ActionRecords and call detect_from_records instead.
pub fn detect(reports: &[TickReport]) -> Vec<InterferenceEvent> {
    let mut events = Vec::new();

    for (i, a) in reports.iter().enumerate() {
        for b in reports.iter().skip(i + 1) {
            let a_interferes_b = a.interference_with.contains(&b.agent_id);
            let b_interferes_a = b.interference_with.contains(&a.agent_id);

            if a_interferes_b && b_interferes_a {
                events.push(InterferenceEvent {
                    tick_sequence: a.tick_sequence,
                    agent_a: a.agent_id,
                    agent_b: b.agent_id,
                    interference_type: InterferenceType::ResourceConflict,
                    severity: 0.8,
                });
                continue;
            }

            if a_interferes_b {
                events.push(InterferenceEvent {
                    tick_sequence: a.tick_sequence,
                    agent_a: a.agent_id,
                    agent_b: b.agent_id,
                    interference_type: InterferenceType::CollateralDamage,
                    severity: 0.5,
                });
                continue;
            }
            if b_interferes_a {
                events.push(InterferenceEvent {
                    tick_sequence: b.tick_sequence,
                    agent_a: b.agent_id,
                    agent_b: a.agent_id,
                    interference_type: InterferenceType::CollateralDamage,
                    severity: 0.5,
                });
                continue;
            }

            if let (Some(action_a), Some(action_b)) = (&a.action_taken, &b.action_taken) {
                let same_kind = action_a.kind == action_b.kind;
                let same_target =
                    action_a.target.is_some() && action_a.target == action_b.target;

                let same_payload = action_a.payload_hash == action_b.payload_hash;
                if same_kind && (same_target || same_payload) {
                    events.push(InterferenceEvent {
                        tick_sequence: a.tick_sequence,
                        agent_a: a.agent_id,
                        agent_b: b.agent_id,
                        interference_type: InterferenceType::Redundancy,
                        severity: 0.2,
                    });
                }
            }
        }
    }

    events
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::cadence::AgentId;

    #[test]
    fn no_records_no_interference() {
        assert!(detect_from_records(1, &[]).is_empty());
    }

    #[test]
    fn disjoint_writes_no_conflict() {
        let a = ActionRecord::new(AgentId::new())
            .with_write("issues:1", serde_json::json!("closed"));
        let b = ActionRecord::new(AgentId::new())
            .with_write("issues:2", serde_json::json!("open"));
        assert!(detect_from_records(1, &[a, b]).is_empty());
    }

    #[test]
    fn same_key_different_value_is_resource_conflict() {
        let a = ActionRecord::new(AgentId::new())
            .with_write("issues:1", serde_json::json!("closed"));
        let b = ActionRecord::new(AgentId::new())
            .with_write("issues:1", serde_json::json!("open"));
        let events = detect_from_records(1, &[a, b]);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0].interference_type, InterferenceType::ResourceConflict));
        assert!((events[0].severity - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn same_key_same_value_is_redundancy() {
        let a = ActionRecord::new(AgentId::new())
            .with_write("issues:1", serde_json::json!("closed"));
        let b = ActionRecord::new(AgentId::new())
            .with_write("issues:1", serde_json::json!("closed"));
        let events = detect_from_records(1, &[a, b]);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0].interference_type, InterferenceType::Redundancy));
    }

    #[test]
    fn side_effect_on_read_set_is_collateral() {
        let agent_a = AgentId::new();
        let agent_b = AgentId::new();
        let a = ActionRecord::new(agent_a).with_side_effect("rate_limit:github", 0.7);
        let b = ActionRecord::new(agent_b).with_read("rate_limit:github");
        let events = detect_from_records(1, &[a, b]);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0].interference_type, InterferenceType::CollateralDamage));
        assert_eq!(events[0].agent_a, agent_a);
        assert_eq!(events[0].agent_b, agent_b);
        assert!((events[0].severity - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn bidirectional_side_effects_produce_two_events() {
        let a = ActionRecord::new(AgentId::new())
            .with_read("zone:alpha")
            .with_side_effect("zone:beta", 0.5);
        let b = ActionRecord::new(AgentId::new())
            .with_read("zone:beta")
            .with_side_effect("zone:alpha", 0.6);
        assert_eq!(detect_from_records(1, &[a, b]).len(), 2);
    }

    #[test]
    fn three_agents_pairwise_conflicts() {
        let a = ActionRecord::new(AgentId::new())
            .with_write("shared:key", serde_json::json!(1));
        let b = ActionRecord::new(AgentId::new())
            .with_write("shared:key", serde_json::json!(2));
        let c = ActionRecord::new(AgentId::new())
            .with_write("shared:key", serde_json::json!(3));
        let events = detect_from_records(1, &[a, b, c]);
        assert_eq!(events.len(), 3); // a-b, a-c, b-c
    }

    fn hist_write(
        key: &str,
        writer: AgentId,
        value: serde_json::Value,
        millis_ago: u64,
    ) -> EnvironmentWrite {
        use std::time::{Duration, Instant};
        EnvironmentWrite {
            key: crate::types::WorkspaceKey::from(key),
            writer,
            value,
            timestamp: Instant::now() - Duration::from_millis(millis_ago),
        }
    }

    #[test]
    fn negation_null_clear_detected() {
        let a = AgentId::new();
        let b = AgentId::new();
        // History: b wrote K=1 200ms ago, then b wrote K=2 100ms ago.
        let history = vec![
            hist_write("k", b, serde_json::json!(1), 300),
            hist_write("k", b, serde_json::json!(2), 200),
        ];
        // Current tick: a clears k to null.
        let rec = ActionRecord::new(a).with_write("k", serde_json::json!(null));
        let events = detect_from_records_with_history(5, &[rec], &history);
        assert!(events
            .iter()
            .any(|e| matches!(e.interference_type, InterferenceType::ActionNegation)
                && e.agent_a == a
                && e.agent_b == b));
    }

    #[test]
    fn negation_revert_to_prior_value_detected() {
        let a = AgentId::new();
        let b = AgentId::new();
        // History: a wrote K=1 earliest, then b wrote K=2 later.
        let history = vec![
            hist_write("k", a, serde_json::json!(1), 400),
            hist_write("k", b, serde_json::json!(2), 200),
        ];
        // Current tick: a reverts K back to 1 — this negates b's K=2.
        let rec = ActionRecord::new(a).with_write("k", serde_json::json!(1));
        let events = detect_from_records_with_history(5, &[rec], &history);
        assert!(events
            .iter()
            .any(|e| matches!(e.interference_type, InterferenceType::ActionNegation)
                && e.agent_a == a
                && e.agent_b == b));
    }

    #[test]
    fn same_value_as_b_is_not_negation() {
        let a = AgentId::new();
        let b = AgentId::new();
        // History: b wrote K=2.
        let history = vec![hist_write("k", b, serde_json::json!(2), 200)];
        // Current tick: a writes K=2 — same as b, redundant but not negation.
        let rec = ActionRecord::new(a).with_write("k", serde_json::json!(2));
        let events = detect_from_records_with_history(5, &[rec], &history);
        assert!(!events
            .iter()
            .any(|e| matches!(e.interference_type, InterferenceType::ActionNegation)));
    }

    #[test]
    fn no_prior_writer_no_negation() {
        let a = AgentId::new();
        // History: a wrote K=1 earlier (same agent, doesn't count).
        let history = vec![hist_write("k", a, serde_json::json!(1), 200)];
        // Current tick: a writes K=null — no other writer to negate.
        let rec = ActionRecord::new(a).with_write("k", serde_json::json!(null));
        let events = detect_from_records_with_history(5, &[rec], &history);
        assert!(!events
            .iter()
            .any(|e| matches!(e.interference_type, InterferenceType::ActionNegation)));
    }

    #[test]
    fn detect_preflagged_mutual_is_resource_conflict() {
        use std::time::Duration;
        let a = AgentId::new();
        let b = AgentId::new();
        let reports = vec![
            TickReport {
                agent_id: a, tick_sequence: 1, action_taken: None,
                latency: Duration::from_millis(0), intent_alignment: 1.0,
                interference_with: vec![b],
            },
            TickReport {
                agent_id: b, tick_sequence: 1, action_taken: None,
                latency: Duration::from_millis(0), intent_alignment: 1.0,
                interference_with: vec![a],
            },
        ];
        let events = detect(&reports);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0].interference_type, InterferenceType::ResourceConflict));
    }
}
