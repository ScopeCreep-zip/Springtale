//! GeneralAgent — default role with full access to all connector capabilities.
//!
//! Per DRG: shared baseline + unique specialization per class.
//! The general agent can execute any action its capabilities allow.

use crate::cadence::ActionDescriptor;
use crate::capability::CapabilityDecl;

use super::trait_::DynamicRoleTrait;

/// Default role — agent has full access to all its connector capabilities.
/// Assigned at formation spawn time. Stays until a transformation (§14)
/// swaps it for a specialized role.
#[derive(Debug, Clone)]
pub struct GeneralAgent {
    capabilities: Vec<CapabilityDecl>,
}

impl GeneralAgent {
    pub fn new(capabilities: Vec<CapabilityDecl>) -> Self {
        Self { capabilities }
    }
}

impl DynamicRoleTrait for GeneralAgent {
    fn name(&self) -> &str {
        "General"
    }

    fn can_execute(&self, _action: &ActionDescriptor) -> bool {
        true
    }

    fn capabilities(&self) -> &[CapabilityDecl] {
        &self.capabilities
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn general_agent_allows_everything() {
        let agent = GeneralAgent::new(vec!["github".into(), "slack".into()]);
        let action = ActionDescriptor {
            kind: "delete_repo".to_owned(),
            target: Some("my-repo".to_owned()),
            payload_hash: 0,
        };
        assert!(agent.can_execute(&action));
    }

    #[test]
    fn general_agent_exposes_all_capabilities() {
        let caps: Vec<CapabilityDecl> = vec!["github".into(), "slack".into()];
        let agent = GeneralAgent::new(caps.clone());
        assert_eq!(agent.capabilities().len(), 2);
        assert_eq!(agent.capabilities()[0].name, "github");
    }

    #[test]
    fn name_is_general() {
        let agent = GeneralAgent::new(vec![]);
        assert_eq!(agent.name(), "General");
    }
}
