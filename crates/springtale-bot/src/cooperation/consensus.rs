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

use super::cadence::AgentId;

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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_majority_vote() {
        let mut vote = ConsensusVote {
            question: DecisionDescriptor {
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
}
