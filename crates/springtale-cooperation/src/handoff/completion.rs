//! Handoff completion — the event the momentum window counts.
//!
//! COOPERATION.pdf §20: "The handoff point is where most cooperative
//! failures occur." Plan 1.3 gives [`crate::momentum::RunWindow`] a
//! `handoffs` / `handoffs_ok` pair and a `handoff_rate`, but nothing
//! emitted a completion, so the rate was always zero and promotion could
//! not see the place failures actually happen.
//!
//! Every dispatch through `Formation::dispatch_handoff` now records one
//! [`HandoffCompletion`] here. The tick drains the log, counts it into
//! the window and re-emits each record on the cooperation event stream.

use std::sync::Mutex;

use crate::cadence::AgentId;

use super::HandoffType;
use super::transfer::HandoffResult;

/// One finished handoff: which pattern, between whom, and whether the
/// work product actually landed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandoffCompletion {
    /// `"direct"`, `"environment_mediated"`, `"flexible_chain"`,
    /// `"sequential_dependency"` or `"information_transfer"`.
    pub pattern: &'static str,
    /// The agent that handed the work over.
    pub from: AgentId,
    /// The agent that received it, when the pattern names one. An
    /// environment-mediated deposit and a flexible-chain step do not.
    pub to: Option<AgentId>,
    /// False for [`HandoffResult::Failed`] — a missing substrate, an
    /// unroutable payload, a store error.
    pub success: bool,
}

impl HandoffType {
    /// Stable name of the handoff pattern, for events and logs.
    pub fn pattern(&self) -> &'static str {
        match self {
            Self::Direct { .. } => "direct",
            Self::EnvironmentMediated { .. } => "environment_mediated",
            Self::FlexibleChain { .. } => "flexible_chain",
            Self::SequentialDependency { .. } => "sequential_dependency",
            Self::InformationTransfer { .. } => "information_transfer",
        }
    }

    /// Who handed the work over.
    pub fn from(&self) -> AgentId {
        match self {
            Self::Direct { sender, .. } => *sender,
            Self::EnvironmentMediated { depositor, .. } => *depositor,
            Self::FlexibleChain { originator, .. } => *originator,
            Self::SequentialDependency { enabler, .. } => *enabler,
            Self::InformationTransfer { source, .. } => *source,
        }
    }

    /// Who receives it, when the pattern names exactly one agent.
    pub fn to(&self) -> Option<AgentId> {
        match self {
            Self::Direct { receiver, .. } => Some(*receiver),
            Self::SequentialDependency { enabled, .. } => Some(*enabled),
            Self::EnvironmentMediated { .. }
            | Self::FlexibleChain { .. }
            | Self::InformationTransfer { .. } => None,
        }
    }
}

impl HandoffResult {
    /// Whether the work product reached its substrate.
    pub fn succeeded(&self) -> bool {
        !matches!(self, Self::Failed(_))
    }
}

/// Completions since the last drain. One per formation, shared behind an
/// `Arc` because `dispatch_handoff` takes `&self`.
#[derive(Debug, Default)]
pub struct HandoffLog {
    completions: Mutex<Vec<HandoffCompletion>>,
}

impl HandoffLog {
    /// Record one finished handoff. A poisoned lock drops the record
    /// rather than propagating a panic into the dispatch path: an
    /// unmeasured handoff is a worse outcome than a lost one only for
    /// the statistics, never for the work.
    pub fn record(&self, handoff: &HandoffType, result: &HandoffResult) {
        let completion = HandoffCompletion {
            pattern: handoff.pattern(),
            from: handoff.from(),
            to: handoff.to(),
            success: result.succeeded(),
        };
        match self.completions.lock() {
            Ok(mut log) => log.push(completion),
            Err(_) => tracing::warn!("handoff log poisoned; completion not counted"),
        }
    }

    /// Take everything recorded since the last call. Called once per
    /// tick by the momentum step.
    pub fn drain(&self) -> Vec<HandoffCompletion> {
        match self.completions.lock() {
            Ok(mut log) => std::mem::take(&mut *log),
            Err(_) => Vec::new(),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::cadence::ActionDescriptor;
    use crate::routing::types::TaskId;

    fn obligation(enabler: AgentId, enabled: AgentId) -> HandoffType {
        HandoffType::SequentialDependency {
            enabler,
            enabled,
            return_obligation: ActionDescriptor {
                kind: "boost".into(),
                target: None,
                payload_hash: 0,
            },
        }
    }

    #[test]
    fn test_record_then_drain_keeps_outcome_and_empties_the_log() {
        let log = HandoffLog::default();
        let (a, b) = (AgentId::new(), AgentId::new());
        let handoff = obligation(a, b);
        log.record(
            &handoff,
            &HandoffResult::ObligationRegistered {
                enabler: a,
                enabled: b,
                obligation: ActionDescriptor {
                    kind: "boost".into(),
                    target: None,
                    payload_hash: 0,
                },
            },
        );
        log.record(&handoff, &HandoffResult::Failed("no substrate".into()));

        let drained = log.drain();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].pattern, "sequential_dependency");
        assert_eq!(drained[0].from, a);
        assert_eq!(drained[0].to, Some(b));
        assert!(drained[0].success);
        assert!(!drained[1].success);
        assert!(log.drain().is_empty(), "a drain empties the log");
    }

    #[test]
    fn test_succeeded_is_false_only_for_failed() {
        assert!(!HandoffResult::Failed("x".into()).succeeded());
        assert!(
            HandoffResult::Deposited {
                location: "k".into()
            }
            .succeeded()
        );
        assert!(
            HandoffResult::Delivered {
                from: AgentId::new(),
                to: AgentId::new(),
                task_id: TaskId::new_v4(),
            }
            .succeeded()
        );
    }
}
