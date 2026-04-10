//! Composer — pre-mission army selection.
//!
//! Per COOPERATION.pdf §3.1:
//! Game source: Patapon army selection, Total War recruitment,
//! Siege operator pick, Deep Rock class selection.
//!
//! "The composer selects which agents participate and assigns initial
//! role hints. This happens BEFORE execution. Like Patapon's army
//! composition screen, the composer decides what capabilities are
//! available. What happens once they're in the field is cooperation."

use crate::cooperation::cadence::AgentId;
use crate::cooperation::formation::{FormationConstraints, FormationId};
use crate::cooperation::cadence::IntentPattern;

/// Pre-mission composition — which agents form a group.
///
/// From COOPERATION.pdf §3.1:
/// ```text
/// pub struct FormationComposition {
///     pub formation_id: FormationId,
///     pub members: Vec<AgentSlot>,
///     pub intent: IntentPattern,
///     pub constraints: FormationConstraints,
/// }
/// ```
pub struct FormationComposition {
    pub formation_id: FormationId,
    pub members: Vec<AgentSlot>,
    pub intent: IntentPattern,
    pub constraints: FormationConstraints,
}

/// A slot for an agent in a formation composition.
///
/// From COOPERATION.pdf §3.1:
/// ```text
/// pub struct AgentSlot {
///     pub agent_id: AgentId,
///     pub capabilities: Vec<CapabilityDecl>,
///     pub role_hint: Option<RoleHint>,  // suggestion, not mandate
/// }
/// ```
///
/// "The role_hint is like equipping a Patapon with fire arrows —
/// it biases behavior without mandating it."
pub struct AgentSlot {
    pub agent_id: AgentId,
    pub capabilities: Vec<String>,
    pub role_hint: Option<RoleHint>,
    /// Per-agent AI adapter config. Read from config store key `ai:{agent_id}`
    /// at formation assembly time. The composer passes this to the factory
    /// to create a per-agent adapter during formation launch.
    pub ai_config: Option<serde_json::Value>,
}

/// A hint about what role this agent should gravitate toward.
/// Suggestion, not mandate. Per §23: bias, not lock.
pub enum RoleHint {
    /// Agent should focus on primary task execution.
    Primary,
    /// Agent should focus on support/monitoring.
    Support,
    /// Agent should focus on information gathering.
    Information,
    /// Custom role hint.
    Custom(String),
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_composition() {
        let comp = FormationComposition {
            formation_id: FormationId::new(),
            members: vec![
                AgentSlot {
                    agent_id: AgentId::new(),
                    capabilities: vec!["slack_send".into()],
                    role_hint: Some(RoleHint::Primary),
                    ai_config: None,
                },
                AgentSlot {
                    agent_id: AgentId::new(),
                    capabilities: vec!["github_read".into()],
                    role_hint: Some(RoleHint::Information),
                    ai_config: None,
                },
            ],
            intent: IntentPattern::Reconnoiter { target: "open_issues".into() },
            constraints: FormationConstraints::default(),
        };
        assert_eq!(comp.members.len(), 2);
    }
}
