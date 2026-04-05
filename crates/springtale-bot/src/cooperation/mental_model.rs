//! Shared mental model — accumulated context that enables anticipatory cooperation.
//!
//! Per COOPERATION.pdf §21:
//! "Teams that share a mental model cooperate with less communication
//! overhead. The model must be built, not assumed."
//!
//! Game patterns:
//! - L4D: Cultural preload (everyone knows zombie movies)
//! - Overcooked: Real-world knowledge as model (everyone knows cooking)
//! - Siege: Accumulated map knowledge (callout vocabulary)
//! - MH: Pattern recognition (monster attack windows)
//! - Patapon: Rhythm as universal model (4-beat measure)
//! - Total War: Doctrinal knowledge (Sun Tzu's Art of War)
//! - DRG: Class capability awareness ("I know what you can do")

use std::collections::HashMap;
use std::time::Instant;

use super::cadence::AgentId;

/// What a formation has collectively learned.
///
/// From COOPERATION.pdf §21.2:
pub struct SharedMentalModel {
    /// What the formation has learned about its task domain.
    /// Grows over time. MH: monster patterns. Siege: map knowledge.
    pub domain_knowledge: HashMap<String, DomainEntry>,

    /// What each agent knows about other agents' capabilities.
    /// DRG: "Engineer has platforms." Siege: "Thermite has hard breach."
    pub capability_awareness: HashMap<AgentId, Vec<String>>,

    /// Successful patterns the formation has used before.
    /// MH: "when monster topples, hammer goes to head, cutter goes to tail."
    pub cooperation_patterns: Vec<CooperationPattern>,

    /// Vocabulary for structured communication.
    /// Siege: room names. MH: monster part names.
    pub shared_vocabulary: HashMap<String, VocabularyEntry>,

    /// Formation-specific conventions that emerged from experience.
    /// "In this formation, agent A usually handles X while agent B handles Y."
    pub conventions: Vec<Convention>,
}

impl Default for SharedMentalModel {
    fn default() -> Self {
        Self {
            domain_knowledge: HashMap::new(),
            capability_awareness: HashMap::new(),
            cooperation_patterns: Vec::new(),
            shared_vocabulary: HashMap::new(),
            conventions: Vec::new(),
        }
    }
}

/// A piece of domain knowledge.
pub struct DomainEntry {
    pub description: String,
    pub learned_at: Instant,
    pub confidence: f32,
}

/// A cooperation pattern the formation has used successfully.
///
/// From COOPERATION.pdf §21.2:
pub struct CooperationPattern {
    pub trigger: String,
    pub participants: Vec<AgentId>,
    pub success_count: u32,
    pub failure_count: u32,
    pub last_used: Instant,
}

/// A shared vocabulary term.
pub struct VocabularyEntry {
    pub term: String,
    pub meaning: String,
    pub established_by: Vec<AgentId>,
}

/// A formation-specific convention that emerged from experience.
///
/// From COOPERATION.pdf §21.2:
pub struct Convention {
    pub description: String,
    pub established_by: Vec<AgentId>,
    pub strength: f32, // how consistently this convention has been followed
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_mental_model_default() {
        let model = SharedMentalModel::default();
        assert!(model.domain_knowledge.is_empty());
        assert!(model.cooperation_patterns.is_empty());
        assert!(model.conventions.is_empty());
    }

    #[test]
    fn test_capability_awareness() {
        let mut model = SharedMentalModel::default();
        let agent = AgentId::new();
        model.capability_awareness.insert(agent, vec!["slack_send".into(), "github_read".into()]);
        assert_eq!(model.capability_awareness.get(&agent).unwrap().len(), 2);
    }
}
