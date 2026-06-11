//! Step 11 — resolve ready consensus votes and APPLY their resolutions
//! (`COOPERATION.md §11`, §5.5 source 2).
//!
//! Supersedes the old `consensus_deadlines` step, which discarded the
//! resolution and only logged it. Every resolved vote now lands on its
//! typed [`DecisionSubject`]:
//!
//! - `DestructiveAction` + approve → a one-shot execution permit is
//!   minted into `formation.consensus_approved`; the task is still on
//!   the blackboard (its claim was released when the vote opened), so
//!   the next capable agent claims it and the executor honors the
//!   permit. Deny **or timeout** → the task is removed from the
//!   blackboard entirely. Default-safe: a destructive action never
//!   executes off a no-quorum timeout, even if the few ballots cast
//!   leaned approve (the engine stays As-Dusk-Falls-faithful — most
//!   popular wins on timeout — but this step applies the §3.3 safety
//!   policy on top for destructive subjects).
//! - `IntentChange` + approve → the formation's intent is replaced and
//!   rebroadcast on the context watch channel; momentum records an
//!   `IntentChanged` event (consecutive-success run resets per §7).
//!   Joint Intention Theory: the joint goal changed by mutual belief.
//!   Deny/timeout → no-op.
//!
//! Vote options are `["approve", "deny"]` by §11 convention —
//! `VoteChoice::Option(0)` is approve.

use tokio::sync::broadcast;

use crate::cooperation::blackboard::trait_::Blackboard;
use crate::cooperation::formation::Formation;
use springtale_cooperation::consensus::{DecisionSubject, VoteChoice, VoteResolution};
use springtale_cooperation::events::{
    self, CooperationEvent, CooperationEventEnvelope, VoteOutcome,
};

/// Index of the "approve" option in the canonical §11 two-option vote.
const APPROVE: usize = 0;

pub fn run(
    formation: &mut Formation,
    cooperation_tx: Option<&broadcast::Sender<CooperationEventEnvelope>>,
) {
    let resolved = formation.consensus.resolve_ready();
    for (vote_id, descriptor, resolution) in resolved {
        match descriptor.subject {
            DecisionSubject::DestructiveAction { task } => {
                formation.awaiting_consensus.remove(&task.id);
                let outcome = destructive_outcome(&resolution);
                match outcome {
                    VoteOutcome::Approved => {
                        // One-shot permit — consumed by the executor on
                        // the next claim of this task.
                        formation.consensus_approved.insert(task.id);
                        tracing::info!(
                            formation = %formation.id.0,
                            vote_id = %vote_id,
                            task = %task.id,
                            "consensus approved destructive action — permit minted"
                        );
                    }
                    VoteOutcome::Denied | VoteOutcome::Timeout => {
                        formation.blackboard.remove_task(&task.id.to_string());
                        tracing::info!(
                            formation = %formation.id.0,
                            vote_id = %vote_id,
                            task = %task.id,
                            outcome = ?outcome,
                            "consensus rejected destructive action — task removed"
                        );
                    }
                }
                events::emit(
                    cooperation_tx,
                    CooperationEvent::ConsensusVoteResolved {
                        formation_id: formation.id,
                        vote_id,
                        outcome,
                    },
                );
            }
            DecisionSubject::IntentChange { proposed } => {
                let outcome = governance_outcome(&resolution);
                if matches!(outcome, VoteOutcome::Approved) {
                    crate::orchestrator::intent::apply_intent(formation, proposed);
                    tracing::info!(
                        formation = %formation.id.0,
                        vote_id = %vote_id,
                        "consensus approved intent change — joint goal updated"
                    );
                }
                events::emit(
                    cooperation_tx,
                    CooperationEvent::ConsensusVoteResolved {
                        formation_id: formation.id,
                        vote_id,
                        outcome,
                    },
                );
            }
        }
    }
}

/// Destructive subjects: only an explicit quorum majority or an override
/// for "approve" passes. Timeout is ALWAYS a denial regardless of the
/// most-popular tally — built for the most vulnerable user.
fn destructive_outcome(resolution: &VoteResolution) -> VoteOutcome {
    match resolution {
        VoteResolution::Majority(VoteChoice::Option(APPROVE))
        | VoteResolution::Override {
            choice: VoteChoice::Option(APPROVE),
            ..
        } => VoteOutcome::Approved,
        VoteResolution::Timeout(_) => VoteOutcome::Timeout,
        _ => VoteOutcome::Denied,
    }
}

/// Governance subjects (intent change): non-destructive, so the
/// As-Dusk-Falls timeout rule applies as the engine resolved it — the
/// most popular choice wins when the deadline passes.
fn governance_outcome(resolution: &VoteResolution) -> VoteOutcome {
    match resolution {
        VoteResolution::Majority(VoteChoice::Option(APPROVE))
        | VoteResolution::Override {
            choice: VoteChoice::Option(APPROVE),
            ..
        }
        | VoteResolution::Timeout(VoteChoice::Option(APPROVE)) => VoteOutcome::Approved,
        VoteResolution::Timeout(_) => VoteOutcome::Timeout,
        _ => VoteOutcome::Denied,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::cooperation::formation::{Formation, FormationMember};
    use springtale_cooperation::action::SubTask;
    use springtale_cooperation::cadence::{AgentId, IntentPattern};
    use springtale_cooperation::consensus::{DecisionDescriptor, DecisionSubject};
    use springtale_cooperation::types::FormationConstraints;

    fn formation_with_vote(approve_votes: usize) -> (Formation, uuid::Uuid, uuid::Uuid) {
        let members: Vec<FormationMember> = (0..3)
            .map(|_| FormationMember::new(AgentId::new(), vec!["shell".into()]))
            .collect();
        let voters: Vec<AgentId> = members.iter().map(|m| m.agent_id).collect();
        let mut f = Formation::new_disconnected(
            members,
            IntentPattern::Execute { plan_id: None },
            FormationConstraints::default(),
        );
        let task = SubTask {
            id: uuid::Uuid::new_v4(),
            target_connector: "shell".into(),
            action_name: "rm".into(),
            params: serde_json::json!({}),
            priority: 1,
            assigned_to: None,
            description: "destructive".into(),
            depends_on: vec![],
        };
        let task_id = task.id;
        let vote_id = f.consensus.propose(
            DecisionDescriptor {
                description: "execute shell::rm".into(),
                options: vec!["approve".into(), "deny".into()],
                required_participants: 3,
                subject: DecisionSubject::DestructiveAction { task },
            },
            std::time::Duration::from_secs(60),
            &voters,
            1,
        );
        f.awaiting_consensus.insert(task_id, vote_id);
        for voter in voters.iter().take(approve_votes) {
            f.consensus
                .vote(&vote_id, *voter, VoteChoice::Option(0))
                .unwrap();
        }
        for voter in voters.iter().skip(approve_votes) {
            f.consensus
                .vote(&vote_id, *voter, VoteChoice::Option(1))
                .unwrap();
        }
        (f, task_id, vote_id)
    }

    #[test]
    fn approved_destructive_vote_mints_one_shot_permit() {
        let (mut f, task_id, _vote_id) = formation_with_vote(3);
        run(&mut f, None);
        assert!(
            f.consensus_approved.contains(&task_id),
            "approval mints the execution permit"
        );
        assert!(
            !f.awaiting_consensus.contains_key(&task_id),
            "guard entry cleared on resolution"
        );
        assert_eq!(f.consensus.active_count(), 0, "vote swept");
    }

    #[test]
    fn denied_destructive_vote_drops_task_without_permit() {
        let (mut f, task_id, _vote_id) = formation_with_vote(1);
        run(&mut f, None);
        assert!(
            !f.consensus_approved.contains(&task_id),
            "denied vote must not mint a permit"
        );
        assert!(!f.awaiting_consensus.contains_key(&task_id));
        assert_eq!(f.consensus.active_count(), 0);
    }

    #[test]
    fn destructive_timeout_is_never_approved() {
        // Even a timeout whose most-popular tally leans approve denies
        // the destructive action (default-safe).
        let r = VoteResolution::Timeout(VoteChoice::Option(APPROVE));
        assert!(matches!(destructive_outcome(&r), VoteOutcome::Timeout));
    }

    #[test]
    fn destructive_majority_approve_passes() {
        let r = VoteResolution::Majority(VoteChoice::Option(APPROVE));
        assert!(matches!(destructive_outcome(&r), VoteOutcome::Approved));
    }

    #[test]
    fn destructive_override_deny_denies() {
        let r = VoteResolution::Override {
            by: AgentId::new(),
            choice: VoteChoice::Option(1),
            cost: 1,
        };
        assert!(matches!(destructive_outcome(&r), VoteOutcome::Denied));
    }

    #[test]
    fn governance_timeout_most_popular_approve_passes() {
        let r = VoteResolution::Timeout(VoteChoice::Option(APPROVE));
        assert!(matches!(governance_outcome(&r), VoteOutcome::Approved));
    }
}
