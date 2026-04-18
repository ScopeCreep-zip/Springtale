//! Interference detection — O(N²) pairwise write-set analysis.
//!
//! Per COOPERATION.md §13.3: compare ActionRecords to detect conflicts.
//! Per event sourcing: optimistic concurrency — two writes to same key = conflict.
//! Per LangGraph: reducer conflict = two agents update same state field.
//! Per OT (Operational Transform): same-key same-value = idempotent (redundancy),
//! same-key different-value = conflict.

use crate::cadence::TickReport;

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
