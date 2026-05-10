//! Consensus engine — weighted decision resolution with openraft-style
//! Vote ordering.
//!
//! Per COOPERATION.md §11:
//! Game source: As Dusk Falls voting + override system.
//!
//! "Up to 8 players vote on story choices simultaneously. Timer
//! counts down. Majority wins. Each player can also override the
//! other players and single-handedly choose one of the options."
//!
//! Overrides are visible to all members and cost a scarce resource.
//! Only available at Fever tier (§7 capability table).
//!
//! **Vote ordering (borrowed from openraft, not the crate itself):**
//! every ballot carries a `(term, voter, committed)` tuple. Committed
//! ballots are strictly greater than uncommitted ballots of the same
//! term — this is how override beats majority without quorum. The full
//! openraft Raft state machine is overkill for per-formation votes that
//! complete in seconds; we lift only the ordering pattern.
//! See <https://github.com/databendlabs/openraft/blob/main/openraft/src/vote/mod.rs>.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::cadence::AgentId;

/// Totally-ordered ballot stamp. Used to break ties between concurrent
/// votes and to make overrides deterministically beat quorum votes.
///
/// Ordering: `term` ascending, then `committed` (true > false), then
/// `voter` uuid. A committed ballot at term T strictly dominates any
/// uncommitted ballot at term T, regardless of voter — override semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ballot {
    pub term: u64,
    pub voter: AgentId,
    pub committed: bool,
}

impl Ord for Ballot {
    fn cmp(&self, other: &Self) -> Ordering {
        self.term
            .cmp(&other.term)
            .then(self.committed.cmp(&other.committed))
            .then(self.voter.0.cmp(&other.voter.0))
    }
}

impl PartialOrd for Ballot {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A decision being voted on by the formation.
///
/// `Serialize + Deserialize` per plan §10.5 — votes are a wire contract
/// across formation members. Deadline is `DateTime<Utc>` (wall-clock)
/// rather than `Instant` (local-monotonic) so the deadline survives
/// serialization to peers and to the audit log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusVote {
    pub question: DecisionDescriptor,
    /// Monotonic term assigned by the ConsensusEngine at propose() time.
    /// All ballots cast on this vote share this term.
    pub term: u64,
    /// All cast ballots indexed by voter. Each entry pairs the Ballot
    /// stamp (for ordering) with the VoteChoice (for tally).
    pub ballots: HashMap<AgentId, (Ballot, VoteChoice)>,
    pub deadline: DateTime<Utc>,
    pub overrides_remaining: HashMap<AgentId, u32>,
    /// Highest-ordered committed ballot seen, if any. A committed ballot
    /// wins immediately on `resolve()` regardless of quorum progress.
    pub committed: Option<(Ballot, VoteChoice)>,
}

/// What's being decided.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionDescriptor {
    pub description: String,
    pub options: Vec<String>,
    /// Minimum votes needed for a valid decision.
    pub required_participants: u32,
}

/// An agent's vote.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VoteChoice {
    /// Vote for a specific option (by index).
    Option(usize),
    /// Abstain from voting.
    Abstain,
}

/// How the vote was resolved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VoteResolution {
    /// Majority of votes selected this choice.
    Majority(VoteChoice),
    /// An agent used their override (scarce resource).
    Override {
        by: AgentId,
        choice: VoteChoice,
        cost: u32,
    },
    /// Deadline expired — most popular choice wins.
    Timeout(VoteChoice),
}

impl ConsensusVote {
    /// Cast an uncommitted vote. Replaces any prior ballot by the same voter.
    pub fn vote(&mut self, agent_id: AgentId, choice: VoteChoice) {
        let ballot = Ballot {
            term: self.term,
            voter: agent_id,
            committed: false,
        };
        self.ballots.insert(agent_id, (ballot, choice));
    }

    /// Attempt an override. The override ballot is committed (Raft
    /// semantics: strictly dominates uncommitted ballots at the same term),
    /// so the vote resolves immediately in the override's favor.
    ///
    /// Returns the resolution on success, or a ConsensusError if the voter
    /// has no override tokens remaining.
    pub fn try_override(
        &mut self,
        agent_id: AgentId,
        choice: VoteChoice,
    ) -> Result<VoteResolution, crate::error::ConsensusError> {
        let remaining = self
            .overrides_remaining
            .get_mut(&agent_id)
            .ok_or(crate::error::ConsensusError::NoOverrideTokens(agent_id))?;
        if *remaining == 0 {
            return Err(crate::error::ConsensusError::NoOverrideTokens(agent_id));
        }
        *remaining -= 1;

        let ballot = Ballot {
            term: self.term,
            voter: agent_id,
            committed: true,
        };

        // Vote ordering: record this committed ballot if it outranks any
        // prior committed ballot. Since committed > uncommitted at same
        // term, any committed ballot wins over all uncommitted peers.
        let replace = self
            .committed
            .as_ref()
            .is_none_or(|(existing, _)| ballot > *existing);
        if replace {
            self.committed = Some((ballot, choice.clone()));
        }
        self.ballots.insert(agent_id, (ballot, choice.clone()));

        Ok(VoteResolution::Override {
            by: agent_id,
            choice,
            cost: 1,
        })
    }

    /// Resolve the vote. Returns `None` if the vote is still open
    /// (no quorum, no override, deadline not reached).
    ///
    /// Priority: committed ballot wins immediately, else quorum, else
    /// timeout. Non-None return means the vote is complete and may be
    /// removed from the active set.
    pub fn resolve(&self) -> Option<VoteResolution> {
        if let Some((ballot, choice)) = &self.committed {
            return Some(VoteResolution::Override {
                by: ballot.voter,
                choice: choice.clone(),
                cost: 1,
            });
        }

        if Utc::now() >= self.deadline {
            return Some(VoteResolution::Timeout(self.winner()));
        }

        if self.ballots.len() >= self.question.required_participants as usize {
            return Some(VoteResolution::Majority(self.winner()));
        }

        None
    }

    /// Force-resolve regardless of quorum — used by deadline sweeper.
    pub fn resolve_expired(&self) -> VoteResolution {
        if let Some((ballot, choice)) = &self.committed {
            return VoteResolution::Override {
                by: ballot.voter,
                choice: choice.clone(),
                cost: 1,
            };
        }
        VoteResolution::Timeout(self.winner())
    }

    fn winner(&self) -> VoteChoice {
        let mut counts: HashMap<&VoteChoice, usize> = HashMap::new();
        for (_ballot, choice) in self.ballots.values() {
            if *choice != VoteChoice::Abstain {
                *counts.entry(choice).or_insert(0) += 1;
            }
        }
        counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(choice, _)| choice.clone())
            .unwrap_or(VoteChoice::Abstain)
    }
}

/// Collection manager for active consensus votes in a formation.
///
/// Assigns monotonic terms per `propose()` call so concurrent votes never
/// share a term — this preserves the Ballot ordering guarantee across
/// the whole formation.
pub struct ConsensusEngine {
    active_votes: HashMap<uuid::Uuid, ConsensusVote>,
    next_term: AtomicU64,
}

impl Default for ConsensusEngine {
    fn default() -> Self {
        Self {
            active_votes: HashMap::new(),
            next_term: AtomicU64::new(1),
        }
    }
}

impl ConsensusEngine {
    /// Create a new empty consensus engine.
    pub fn new() -> Self {
        Self::default()
    }

    /// Propose a new vote. Returns the vote ID.
    pub fn propose(
        &mut self,
        question: DecisionDescriptor,
        deadline: std::time::Duration,
        voters: &[AgentId],
        override_tokens: u32,
    ) -> uuid::Uuid {
        let id = uuid::Uuid::new_v4();
        let overrides = voters.iter().map(|v| (*v, override_tokens)).collect();
        let term = self.next_term.fetch_add(1, AtomicOrdering::SeqCst);
        let vote = ConsensusVote {
            question,
            term,
            ballots: HashMap::new(),
            deadline: Utc::now()
                + chrono::TimeDelta::from_std(deadline)
                    .unwrap_or_else(|_| chrono::TimeDelta::zero()),
            overrides_remaining: overrides,
            committed: None,
        };
        self.active_votes.insert(id, vote);
        id
    }

    /// Cast a vote on an active proposal.
    pub fn vote(
        &mut self,
        vote_id: &uuid::Uuid,
        agent_id: AgentId,
        choice: VoteChoice,
    ) -> Result<(), crate::error::ConsensusError> {
        let vote = self
            .active_votes
            .get_mut(vote_id)
            .ok_or(crate::error::ConsensusError::VoteNotFound(*vote_id))?;
        vote.vote(agent_id, choice);
        Ok(())
    }

    /// Attempt an override on an active proposal. On success the vote is
    /// resolved and removed from the active set.
    pub fn try_override(
        &mut self,
        vote_id: &uuid::Uuid,
        agent_id: AgentId,
        choice: VoteChoice,
    ) -> Result<VoteResolution, crate::error::ConsensusError> {
        let vote = self
            .active_votes
            .get_mut(vote_id)
            .ok_or(crate::error::ConsensusError::VoteNotFound(*vote_id))?;
        let resolution = vote.try_override(agent_id, choice)?;
        self.active_votes.remove(vote_id);
        Ok(resolution)
    }

    /// Poll all active votes. Resolved votes are removed and returned.
    pub fn check_deadlines(&mut self) -> Vec<(uuid::Uuid, VoteResolution)> {
        let resolved_ids: Vec<(uuid::Uuid, VoteResolution)> = self
            .active_votes
            .iter()
            .filter_map(|(id, v)| v.resolve().map(|r| (*id, r)))
            .collect();
        for (id, _) in &resolved_ids {
            self.active_votes.remove(id);
        }
        resolved_ids
    }

    /// Get count of active votes.
    pub fn active_count(&self) -> usize {
        self.active_votes.len()
    }

    /// Borrow an active vote (for introspection / tests).
    pub fn get(&self, vote_id: &uuid::Uuid) -> Option<&ConsensusVote> {
        self.active_votes.get(vote_id)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn vote_shell(term: u64, required: u32, deadline: Duration) -> ConsensusVote {
        ConsensusVote {
            question: DecisionDescriptor {
                required_participants: required,
                description: "test".into(),
                options: vec!["a".into(), "b".into()],
            },
            term,
            ballots: HashMap::new(),
            deadline: Utc::now()
                + chrono::TimeDelta::from_std(deadline)
                    .unwrap_or_else(|_| chrono::TimeDelta::zero()),
            overrides_remaining: HashMap::new(),
            committed: None,
        }
    }

    #[test]
    fn ballot_ordering_committed_beats_uncommitted() {
        let voter = AgentId::new();
        let uncommitted = Ballot {
            term: 5,
            voter,
            committed: false,
        };
        let committed = Ballot {
            term: 5,
            voter,
            committed: true,
        };
        assert!(committed > uncommitted);
    }

    #[test]
    fn ballot_ordering_higher_term_wins() {
        let voter = AgentId::new();
        let low = Ballot {
            term: 3,
            voter,
            committed: true,
        };
        let high = Ballot {
            term: 7,
            voter,
            committed: false,
        };
        assert!(high > low);
    }

    #[test]
    fn majority_vote_resolves_when_quorum_reached() {
        let mut vote = vote_shell(1, 2, Duration::from_secs(60));
        let a = AgentId::new();
        let b = AgentId::new();
        let c = AgentId::new();

        vote.vote(a, VoteChoice::Option(0));
        vote.vote(b, VoteChoice::Option(0));
        vote.vote(c, VoteChoice::Option(1));

        let result = vote.resolve().expect("quorum reached");
        assert!(matches!(
            result,
            VoteResolution::Majority(VoteChoice::Option(0))
        ));
    }

    #[test]
    fn below_quorum_returns_none() {
        let mut vote = vote_shell(1, 3, Duration::from_secs(60));
        let a = AgentId::new();
        vote.vote(a, VoteChoice::Option(0));
        assert!(vote.resolve().is_none());
    }

    #[test]
    fn override_wins_immediately_even_below_quorum() {
        let mut vote = vote_shell(1, 5, Duration::from_secs(60));
        let agent = AgentId::new();
        vote.overrides_remaining.insert(agent, 1);

        let result = vote
            .try_override(agent, VoteChoice::Option(1))
            .expect("override should succeed");
        assert!(matches!(result, VoteResolution::Override { cost: 1, .. }));
        // Committed ballot now recorded — resolve() returns Override.
        assert!(matches!(
            vote.resolve(),
            Some(VoteResolution::Override { .. })
        ));
    }

    #[test]
    fn override_beats_majority_at_same_term() {
        let mut vote = vote_shell(1, 2, Duration::from_secs(60));
        let a = AgentId::new();
        let b = AgentId::new();
        let c = AgentId::new();
        vote.overrides_remaining.insert(c, 1);

        vote.vote(a, VoteChoice::Option(0));
        vote.vote(b, VoteChoice::Option(0));
        // Quorum for option 0 reached, but c overrides to option 1.
        vote.try_override(c, VoteChoice::Option(1)).unwrap();

        let result = vote.resolve().expect("resolvable");
        match result {
            VoteResolution::Override { by, choice, .. } => {
                assert_eq!(by, c);
                assert_eq!(choice, VoteChoice::Option(1));
            }
            other => panic!("expected Override, got {other:?}"),
        }
    }

    #[test]
    fn override_exhausted_returns_error() {
        let mut vote = vote_shell(1, 2, Duration::from_secs(60));
        let agent = AgentId::new();
        vote.overrides_remaining.insert(agent, 0);

        let result = vote.try_override(agent, VoteChoice::Option(0));
        assert!(matches!(
            result,
            Err(crate::error::ConsensusError::NoOverrideTokens(_))
        ));
    }

    #[test]
    fn engine_propose_assigns_monotonic_terms() {
        let mut engine = ConsensusEngine::new();
        let a = AgentId::new();
        let q = || DecisionDescriptor {
            required_participants: 1,
            description: "x".into(),
            options: vec!["y".into()],
        };
        let id1 = engine.propose(q(), Duration::from_secs(60), &[a], 0);
        let id2 = engine.propose(q(), Duration::from_secs(60), &[a], 0);
        let t1 = engine.get(&id1).unwrap().term;
        let t2 = engine.get(&id2).unwrap().term;
        assert!(t2 > t1);
    }

    #[test]
    fn engine_deadline_expiry_resolves_as_timeout() {
        let mut engine = ConsensusEngine::new();
        let a = AgentId::new();

        engine.propose(
            DecisionDescriptor {
                required_participants: 1,
                description: "action".into(),
                options: vec!["go".into()],
            },
            Duration::from_secs(0),
            &[a],
            0,
        );

        // Cast a vote so `winner()` has something to return.
        let id = *engine.active_votes.keys().next().unwrap();
        engine.vote(&id, a, VoteChoice::Option(0)).unwrap();

        let resolved = engine.check_deadlines();
        assert_eq!(resolved.len(), 1);
        assert_eq!(engine.active_count(), 0);
    }

    #[test]
    fn engine_try_override_removes_vote() {
        let mut engine = ConsensusEngine::new();
        let a = AgentId::new();
        let id = engine.propose(
            DecisionDescriptor {
                required_participants: 5,
                description: "decision".into(),
                options: vec!["yes".into(), "no".into()],
            },
            Duration::from_secs(60),
            &[a],
            1,
        );

        let result = engine.try_override(&id, a, VoteChoice::Option(0));
        assert!(matches!(result, Ok(VoteResolution::Override { .. })));
        assert_eq!(engine.active_count(), 0);
    }
}
