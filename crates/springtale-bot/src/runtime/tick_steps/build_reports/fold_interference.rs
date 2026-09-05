//! Fold detected interference events back into the member reports.
//!
//! `tick_processor::process_tick_with_context` detects interference from
//! two sources: `interference_with` pre-flagged on the reports, and the
//! shared-environment write log (pairwise ResourceConflict/Redundancy
//! plus cross-tick ActionNegation). The agent step pipeline builds every
//! report with an empty `interference_with`, so without this pass the
//! write-log events never reach awareness or the mental model — both
//! only ever see the reports. Plan §1.10 (finding 41).

use springtale_cooperation::tick_processor::FormationTickResult;

/// Name each interference partner on both involved reports.
///
/// Idempotent: a partner already pre-flagged on a report (and therefore
/// re-detected by `interference::detector::detect`) is not added twice.
pub fn run(result: &mut FormationTickResult) {
    for ev in &result.interferences {
        for report in result.reports.iter_mut() {
            let partner = if report.agent_id == ev.agent_a {
                ev.agent_b
            } else if report.agent_id == ev.agent_b {
                ev.agent_a
            } else {
                continue;
            };
            if partner != report.agent_id && !report.interference_with.contains(&partner) {
                report.interference_with.push(partner);
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    use springtale_cooperation::cadence::{ActionDescriptor, AgentId, TickReport};
    use springtale_cooperation::state::EnvironmentWrite;
    use springtale_cooperation::tick::TickId;
    use springtale_cooperation::tick_processor;
    use springtale_cooperation::types::WorkspaceKey;

    fn report(agent: AgentId) -> TickReport {
        TickReport {
            agent_id: agent,
            tick_sequence: TickId(1),
            action_taken: Some(ActionDescriptor {
                kind: "write".to_owned(),
                target: Some("shared/key".to_owned()),
                payload_hash: 0,
            }),
            latency: Duration::from_millis(1),
            intent_alignment: 0.9,
            interference_with: vec![],
        }
    }

    fn write(agent: AgentId, value: &str) -> EnvironmentWrite {
        EnvironmentWrite {
            key: WorkspaceKey::from("shared/key"),
            writer: agent,
            value: serde_json::json!(value),
            timestamp: Instant::now(),
        }
    }

    #[test]
    fn test_run_same_key_writes_name_each_other_in_both_reports() {
        let a = AgentId::new();
        let b = AgentId::new();
        let writes = vec![write(a, "from-a"), write(b, "from-b")];
        let records = tick_processor::action_records_from_writes(&writes);
        let mut result =
            tick_processor::process_tick_with_context(vec![report(a), report(b)], records, &[]);
        assert!(
            result
                .interferences
                .iter()
                .any(|ev| (ev.agent_a == a && ev.agent_b == b)
                    || (ev.agent_a == b && ev.agent_b == a)),
            "write-log conflict must be detected before folding"
        );
        assert!(
            result
                .reports
                .iter()
                .all(|r| r.interference_with.is_empty())
        );

        run(&mut result);

        let of = |id: AgentId| {
            result
                .reports
                .iter()
                .find(|r| r.agent_id == id)
                .unwrap()
                .interference_with
                .clone()
        };
        assert_eq!(of(a), vec![b]);
        assert_eq!(of(b), vec![a]);
    }

    #[test]
    fn test_run_preflagged_partner_not_duplicated() {
        let a = AgentId::new();
        let b = AgentId::new();
        let mut ra = report(a);
        ra.interference_with.push(b);
        let mut result = tick_processor::process_tick(vec![ra, report(b)]);
        run(&mut result);
        run(&mut result);
        let of = |id: AgentId| {
            result
                .reports
                .iter()
                .find(|r| r.agent_id == id)
                .unwrap()
                .interference_with
                .clone()
        };
        assert_eq!(of(a), vec![b]);
        assert_eq!(of(b), vec![a]);
    }
}
