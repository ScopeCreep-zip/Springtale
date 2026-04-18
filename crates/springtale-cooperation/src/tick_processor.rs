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
use crate::interference::{self, InterferenceEvent};

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
    let interferences = interference::detector::detect(&member_reports);

    let all_succeeded = !member_reports.is_empty()
        && member_reports.iter().all(|r| r.intent_alignment > 0.5)
        && interferences.is_empty();

    FormationTickResult {
        reports: member_reports,
        interferences,
        all_succeeded,
    }
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
