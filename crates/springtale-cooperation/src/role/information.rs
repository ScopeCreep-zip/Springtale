//! InformationAgent — read-only observation role after primary capability loss.
//!
//! Per Siege: dead players switch to cameras and provide callouts.
//! The agent can't execute actions but can still observe, report, and
//! contribute intelligence to the formation's mental model.

use crate::cadence::ActionDescriptor;
use crate::capability::CapabilityDecl;

use super::trait_::DynamicRoleTrait;

/// Read-only role — agent observes and reports, cannot execute.
/// Assigned when transformation trigger detects incapacitation or death
/// (§14: `RoleTransformation::ToInformationAgent`).
#[derive(Debug, Clone)]
pub struct InformationAgent {
    read_capabilities: Vec<CapabilityDecl>,
}

impl InformationAgent {
    /// Construct from the agent's original capabilities, keeping only
    /// read/observe/monitor capabilities.
    pub fn from_original(original: &[CapabilityDecl]) -> Self {
        let read_capabilities = original
            .iter()
            .filter(|c| {
                let name = c.name.to_lowercase();
                name.contains("read")
                    || name.contains("observe")
                    || name.contains("monitor")
                    || name.contains("list")
                    || name.contains("get")
            })
            .cloned()
            .collect();
        Self { read_capabilities }
    }

    /// Construct with explicit read capabilities.
    pub fn new(read_capabilities: Vec<CapabilityDecl>) -> Self {
        Self { read_capabilities }
    }
}

impl DynamicRoleTrait for InformationAgent {
    fn name(&self) -> &str {
        "Information"
    }

    fn can_execute(&self, action: &ActionDescriptor) -> bool {
        let kind = action.kind.to_lowercase();
        kind.contains("read")
            || kind.contains("observe")
            || kind.contains("monitor")
            || kind.contains("list")
            || kind.contains("get")
    }

    fn capabilities(&self) -> &[CapabilityDecl] {
        &self.read_capabilities
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn blocks_write_actions() {
        let agent = InformationAgent::new(vec![]);
        let write = ActionDescriptor {
            kind: "send_message".to_owned(),
            target: None,
            payload_hash: 0,
        };
        assert!(!agent.can_execute(&write));
    }

    #[test]
    fn allows_read_actions() {
        let agent = InformationAgent::new(vec![]);
        let read = ActionDescriptor {
            kind: "read_issues".to_owned(),
            target: None,
            payload_hash: 0,
        };
        assert!(agent.can_execute(&read));
    }

    #[test]
    fn from_original_filters_to_reads() {
        let original: Vec<CapabilityDecl> = vec![
            "github.read_issues".into(),
            "github.create_issue".into(),
            "slack.send_message".into(),
            "slack.list_channels".into(),
        ];
        let agent = InformationAgent::from_original(&original);
        assert_eq!(agent.capabilities().len(), 2);
        assert!(
            agent
                .capabilities()
                .iter()
                .any(|c| c.name == "github.read_issues")
        );
        assert!(
            agent
                .capabilities()
                .iter()
                .any(|c| c.name == "slack.list_channels")
        );
    }

    #[test]
    fn name_is_information() {
        assert_eq!(InformationAgent::new(vec![]).name(), "Information");
    }
}
