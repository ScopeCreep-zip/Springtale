//! SupportAgent — assist-only role after primary exhaustion.
//!
//! Per Army of Two: low-aggro player shifts to overwatch — covering fire,
//! healing, spotting. Can't take primary actions but enables the team.

use crate::cadence::ActionDescriptor;
use crate::capability::CapabilityDecl;

use super::trait_::DynamicRoleTrait;

/// Support role — agent assists others but doesn't take primary actions.
/// Assigned when transformation trigger detects repeated failures
/// (§14: `RoleTransformation::ToSupportAgent`, 5+ consecutive failures).
#[derive(Debug, Clone)]
pub struct SupportAgent;

impl SupportAgent {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SupportAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl DynamicRoleTrait for SupportAgent {
    fn name(&self) -> &str {
        "Support"
    }

    fn can_execute(&self, action: &ActionDescriptor) -> bool {
        let kind = action.kind.to_lowercase();
        kind.contains("read")
            || kind.contains("monitor")
            || kind.contains("assist")
            || kind.contains("heal")
            || kind.contains("support")
            || kind.contains("notify")
            || kind.contains("alert")
    }

    fn capabilities(&self) -> &[CapabilityDecl] {
        &[]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn blocks_primary_actions() {
        let agent = SupportAgent::new();
        let attack = ActionDescriptor {
            kind: "delete_branch".to_owned(),
            target: None,
            payload_hash: 0,
        };
        assert!(!agent.can_execute(&attack));
    }

    #[test]
    fn allows_support_actions() {
        let agent = SupportAgent::new();
        for kind in [
            "notify_team",
            "alert_channel",
            "read_status",
            "monitor_ci",
            "assist_deploy",
        ] {
            let action = ActionDescriptor {
                kind: kind.to_owned(),
                target: None,
                payload_hash: 0,
            };
            assert!(agent.can_execute(&action), "should allow {kind}");
        }
    }

    #[test]
    fn name_is_support() {
        assert_eq!(SupportAgent::new().name(), "Support");
    }
}
