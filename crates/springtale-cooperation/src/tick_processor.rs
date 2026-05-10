//! Tick processor — per-formation tick processing pipeline.
//!
//! Called once per formation per cadence beat. The caller (bot event loop)
//! builds TickReports from member state — the processor aggregates them,
//! runs interference detection, and returns a result the caller uses to
//! update momentum, awareness, and pacing.
//!
//! Design: the cooperation crate processes reports, not members. Member
//! state (health, attention_load, AI adapter) lives in the bot crate.
//! This keeps cooperation independent of springtale-ai.

use crate::cadence::TickReport;
use crate::interference::{self, ActionRecord, InterferenceEvent};
use crate::state::EnvironmentWrite;

/// Aggregate result of processing one tick for a formation.
pub struct FormationTickResult {
    /// Individual reports from each operational member.
    pub reports: Vec<TickReport>,
    /// Detected interference events between members this tick.
    pub interferences: Vec<InterferenceEvent>,
    /// True when all members succeeded with zero interference.
    pub all_succeeded: bool,
}

/// Process a single formation tick from pre-built member reports.
///
/// Steps:
/// 1. Run interference detection (O(N²) pairwise on reports)
/// 2. Compute aggregate success (all aligned + no interference)
///
/// The caller is responsible for:
/// - Building TickReports from member state (bot-layer)
/// - Using the result to update momentum/awareness/pacing (bot-layer)
pub fn process_tick(member_reports: Vec<TickReport>) -> FormationTickResult {
    process_tick_with_context(member_reports, Vec::new(), &[])
}

/// Process a tick with current-tick action records and prior-tick history.
///
/// Per COOPERATION.md §13.1: ActionNegation (A undid B's prior work)
/// cannot be detected from the current tick's records alone — it requires
/// the prior writes with their timestamps. The caller supplies:
///
/// - `member_reports` — per-agent tick summaries (same as [`process_tick`])
/// - `action_records` — structured read/write/side-effect records for
///   this tick. Typically synthesized from the shared-environment
///   write log entries added since the previous tick boundary; see
///   [`action_records_from_writes`].
/// - `history` — the write log up to (but not including) the current
///   tick's writes. Used for Lamport-ordered negation detection.
///
/// The return includes three classes of interference: preflagged from
/// reports, pairwise from records, and cross-tick ActionNegation.
pub fn process_tick_with_context(
    member_reports: Vec<TickReport>,
    action_records: Vec<ActionRecord>,
    history: &[EnvironmentWrite],
) -> FormationTickResult {
    let mut interferences = interference::detector::detect(&member_reports);

    // History-aware detection also produces the pairwise checks
    // (detect_from_records is called first internally), so combining
    // both sources is the complete view. Dedup is the caller's job if
    // they care about counting unique events rather than events-per-source.
    let record_events =
        interference::detector::detect_from_records_with_history(
            member_reports
                .first()
                .map(|r| r.tick_sequence)
                .unwrap_or(0),
            &action_records,
            history,
        );
    interferences.extend(record_events);

    let all_succeeded = !member_reports.is_empty()
        && member_reports.iter().all(|r| r.intent_alignment > 0.5)
        && interferences.is_empty();

    FormationTickResult {
        reports: member_reports,
        interferences,
        all_succeeded,
    }
}

/// Synthesize `ActionRecord`s from a slice of new `EnvironmentWrite`
/// entries. Groups writes by writer so each agent yields one record
/// covering every key it touched this tick.
pub fn action_records_from_writes(new_writes: &[EnvironmentWrite]) -> Vec<ActionRecord> {
    use std::collections::HashMap;

    let mut by_agent: HashMap<crate::cadence::AgentId, ActionRecord> = HashMap::new();
    for w in new_writes {
        let record = by_agent
            .entry(w.writer)
            .or_insert_with(|| ActionRecord::new(w.writer));
        record.write_set.insert(w.key.clone(), w.value.clone());
    }
    by_agent.into_values().collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::cadence::AgentId;
    use std::time::Duration;

    fn report(agent: AgentId, alignment: f32, interferes: Vec<AgentId>) -> TickReport {
        use crate::cadence::ActionDescriptor;
        TickReport {
            agent_id: agent,
            tick_sequence: 1,
            action_taken: Some(ActionDescriptor {
                kind: "work".to_owned(),
                target: None,
                payload_hash: 0,
            }),
            latency: Duration::from_millis(5),
            intent_alignment: alignment,
            interference_with: interferes,
        }
    }

    #[test]
    fn test_all_succeed() {
        let a = AgentId::new();
        let b = AgentId::new();
        // Different actions — no redundancy interference
        use crate::cadence::ActionDescriptor;
        let mut reports = vec![report(a, 0.9, vec![]), report(b, 0.8, vec![])];
        reports[0].action_taken = Some(ActionDescriptor {
            kind: "read_issues".to_owned(),
            target: Some("repo".to_owned()),
            payload_hash: 0,
        });
        reports[1].action_taken = Some(ActionDescriptor {
            kind: "send_message".to_owned(),
            target: Some("channel".to_owned()),
            payload_hash: 0,
        });
        let result = process_tick(reports);
        assert!(result.all_succeeded);
        assert!(result.interferences.is_empty());
        assert_eq!(result.reports.len(), 2);
    }

    #[test]
    fn test_low_alignment_fails() {
        let a = AgentId::new();
        let b = AgentId::new();
        let result = process_tick(vec![report(a, 0.9, vec![]), report(b, 0.3, vec![])]);
        assert!(!result.all_succeeded);
    }

    #[test]
    fn test_interference_fails() {
        let a = AgentId::new();
        let b = AgentId::new();
        let result = process_tick(vec![report(a, 0.9, vec![b]), report(b, 0.9, vec![a])]);
        assert!(!result.all_succeeded);
        assert_eq!(result.interferences.len(), 1);
    }

    #[test]
    fn test_empty_reports() {
        let result = process_tick(vec![]);
        assert!(!result.all_succeeded);
        assert!(result.interferences.is_empty());
    }
}
