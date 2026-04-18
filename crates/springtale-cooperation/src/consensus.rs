//! Consensus engine — weighted decision resolution.
//!
//! Per COOPERATION.pdf §11:
//! Game source: As Dusk Falls voting + override system.
//!
//! "Up to 8 players vote on story choices simultaneously. Timer
//! counts down. Majority wins. Each player can also override the
//! other players and single-handedly choose one of the options."
//!
//! Overrides are visible to all members and cost a scarce resource.
//! Only available at Fever tier (§7 capability table).

use std::collections::HashMap;
use std::time::Instant;

use crate::cadence::AgentId;

/// A decision being voted on by the formation.
///
/// From COOPERATION.pdf §11:
/// ```text
/// pub struct ConsensusVote {
///     pub question: DecisionDescriptor,
///     pub votes: HashMap<AgentId, VoteChoice>,
///     pub deadline: Instant,
///     pub overrides_remaining: HashMap<AgentId, u32>,
/// }
/// ```
pub struct ConsensusVote {
    pub question: DecisionDescriptor,
    pub votes: HashMap<AgentId, VoteChoice>,
    pub deadline: Instant,
    pub overrides_remaining: HashMap<AgentId, u32>,
}

/// What's being decided.
pub struct DecisionDescriptor {
    pub description: String,
    pub options: Vec<String>,
    /// Minimum votes needed for a valid decision.
    pub required_participants: u32,
}

/// An agent's vote.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum VoteChoice {
    /// Vote for a specific option (by index).
    Option(usize),
    /// Abstain from voting.
    Abstain,
}

/// How the vote was resolved.
///
/// From COOPERATION.pdf §11:
/// ```text
/// pub enum VoteResolution {
///     Majority(VoteChoice),
///     Override { by: AgentId, choice: VoteChoice, cost: u32 },
///     Timeout(VoteChoice),
/// }
/// ```
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
    /// Cast a vote.
    pub fn vote(&mut self, agent_id: AgentId, choice: VoteChoice) {
        self.votes.insert(agent_id, choice);
    }

    /// Attempt an override (costs a scarce resource).
    pub fn try_override(
        &mut self,
        agent_id: AgentId,
        choice: VoteChoice,
    ) -> Option<VoteResolution> {
        let remaining = self.overrides_remaining.get_mut(&agent_id)?;
        if *remaining == 0 {
            return None;
        }
        *remaining -= 1;
        Some(VoteResolution::Override {
            by: agent_id,
            choice,
            cost: 1,
        })
    }

    /// Resolve the vote by majority.
    pub fn resolve(&self) -> VoteResolution {
        let mut counts: HashMap<&VoteChoice, usize> = HashMap::new();
        for choice in self.votes.values() {
            if *choice != VoteChoice::Abstain {
                *counts.entry(choice).or_insert(0) += 1;
            }
        }

        let winner = counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(choice, _)| choice.clone())
            .unwrap_or(VoteChoice::Abstain);

        if Instant::now() >= self.deadline {
            VoteResolution::Timeout(winner)
        } else {
            VoteResolution::Majority(winner)
        }
    }
}

/// Collection manager for active consensus votes in a formation.
///
/// Wraps individual `ConsensusVote` instances. `check_deadlines()` is
/// called per cadence tick to force-resolve expired votes.
#[derive(Default)]
pub struct ConsensusEngine {
    active_votes: HashMap<uuid::Uuid, ConsensusVote>,
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
        let vote = ConsensusVote {
            question,
            votes: HashMap::new(),
            deadline: Instant::now() + deadline,
            overrides_remaining: overrides,
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

    /// Check all active votes for deadline expiry. Returns resolved votes
    /// and removes them from the active set.
    pub fn check_deadlines(&mut self) -> Vec<(uuid::Uuid, VoteResolution)> {
        let now = Instant::now();
        let expired: Vec<uuid::Uuid> = self
            .active_votes
            .iter()
            .filter(|(_, v)| now >= v.deadline)
            .map(|(id, _)| *id)
            .collect();

        let mut resolved = Vec::new();
        for id in expired {
            if let Some(vote) = self.active_votes.remove(&id) {
                resolved.push((id, vote.resolve()));
            }
        }
        resolved
    }

    /// Get count of active votes.
    pub fn active_count(&self) -> usize {
        self.active_votes.len()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_majority_vote() {
        let mut vote = ConsensusVote {
            question: DecisionDescriptor {
                required_participants: 2,
                description: "which connector?".into(),
                options: vec!["slack".into(), "discord".into()],
            },
            votes: HashMap::new(),
            deadline: Instant::now() + Duration::from_secs(60),
            overrides_remaining: HashMap::new(),
        };

        let a = AgentId::new();
        let b = AgentId::new();
        let c = AgentId::new();

        vote.vote(a, VoteChoice::Option(0)); // slack
        vote.vote(b, VoteChoice::Option(0)); // slack
        vote.vote(c, VoteChoice::Option(1)); // discord

        let result = vote.resolve();
        assert!(matches!(
            result,
            VoteResolution::Majority(VoteChoice::Option(0))
        ));
    }

    #[test]
    fn test_override() {
        let agent = AgentId::new();
        let mut vote = ConsensusVote {
            question: DecisionDescriptor {
                required_participants: 2,
                description: "action".into(),
                options: vec!["go".into(), "wait".into()],
            },
            votes: HashMap::new(),
            deadline: Instant::now() + Duration::from_secs(60),
            overrides_remaining: HashMap::from([(agent, 2)]),
        };

        let result = vote.try_override(agent, VoteChoice::Option(1));
        assert!(result.is_some());

        // Override count decreased
        assert_eq!(*vote.overrides_remaining.get(&agent).unwrap(), 1);
    }

    #[test]
    fn test_override_exhausted() {
        let agent = AgentId::new();
        let mut vote = ConsensusVote {
            question: DecisionDescriptor {
                required_participants: 2,
                description: "action".into(),
                options: vec!["go".into()],
            },
            votes: HashMap::new(),
            deadline: Instant::now() + Duration::from_secs(60),
            overrides_remaining: HashMap::from([(agent, 0)]),
        };

        let result = vote.try_override(agent, VoteChoice::Option(0));
        assert!(result.is_none());
    }

    #[test]
    fn test_engine_propose_and_vote() {
        let mut engine = ConsensusEngine::new();
        let a = AgentId::new();
        let b = AgentId::new();

        let id = engine.propose(
            DecisionDescriptor {
                required_participants: 2,
                description: "deploy?".into(),
                options: vec!["yes".into(), "no".into()],
            },
            Duration::from_secs(60),
            &[a, b],
            1,
        );

        assert_eq!(engine.active_count(), 1);
        engine.vote(&id, a, VoteChoice::Option(0)).unwrap();
        engine.vote(&id, b, VoteChoice::Option(0)).unwrap();

        // Not expired yet, so check_deadlines returns nothing
        let resolved = engine.check_deadlines();
        assert!(resolved.is_empty());
        assert_eq!(engine.active_count(), 1);
    }

    #[test]
    fn test_engine_deadline_expiry() {
        let mut engine = ConsensusEngine::new();
        let a = AgentId::new();

        engine.propose(
            DecisionDescriptor {
                required_participants: 1,
                description: "action".into(),
                options: vec!["go".into()],
            },
            Duration::from_secs(0), // instant expiry
            &[a],
            0,
        );

        engine.vote(
            &engine.active_votes.keys().next().copied().unwrap(),
            a,
            VoteChoice::Option(0),
        ).unwrap();

        let resolved = engine.check_deadlines();
        assert_eq!(resolved.len(), 1);
        assert_eq!(engine.active_count(), 0); // removed after resolution
    }
}
