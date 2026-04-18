use crate::cooperation::{FormationConstraints, FormationId};
use crate::cooperation::cadence::{AgentId, IntentPattern};

/// Final composition result — which agents were admitted to the formation.
///
/// Per COOPERATION.pdf §3.1.
#[derive(Debug)]
pub struct FormationComposition {
    pub formation_id: FormationId,
    pub members: Vec<AgentSlot>,
    pub intent: IntentPattern,
    pub constraints: FormationConstraints,
}

/// One admitted agent plus its initial role hint and per-agent AI config.
#[derive(Debug)]
pub struct AgentSlot {
    pub agent_id: AgentId,
    pub capabilities: Vec<springtale_cooperation::capability::CapabilityDecl>,
    pub role_hint: Option<RoleHint>,
    /// Per-agent adapter config, read from the config store key
    /// `ai:{agent_id}` at formation-assembly time.
    pub ai_config: Option<serde_json::Value>,
}

/// Soft role bias — a suggestion, not a mandate (§23).
#[derive(Debug)]
pub enum RoleHint {
    Primary,
    Support,
    Information,
    Custom(String),
}
