//! Role transformation application — swaps the Box<dyn DynamicRoleTrait>.
//!
//! Per OpenAI Swarm: handoff = swap active agent entirely.
//! Per RTS: unit role changes, old role drops, new role takes over.
//! No fallback to old role — full forward replacement.

use crate::capability::CapabilityDecl;
use crate::transformation::RoleTransformation;

use super::general::GeneralAgent;
use super::information::InformationAgent;
use super::support::SupportAgent;
use super::trait_::DynamicRoleTrait;

/// Apply a role transformation — produces a new Box<dyn DynamicRoleTrait>.
///
/// The caller replaces the member's `role` field with the returned value.
/// The old role drops entirely — no fallback, no stack of previous roles.
pub fn apply_transformation(
    current_capabilities: &[CapabilityDecl],
    transformation: &RoleTransformation,
) -> Box<dyn DynamicRoleTrait> {
    match transformation {
        RoleTransformation::ToInformationAgent => {
            Box::new(InformationAgent::from_original(current_capabilities))
        }
        RoleTransformation::ToSupportAgent => Box::new(SupportAgent::new()),
        RoleTransformation::ReassignCapabilities(new_caps) => {
            Box::new(GeneralAgent::new(new_caps.clone()))
        }
    }
}

/// Reconstruct a role from its persisted name string.
///
/// Called by `spawn_formation` when loading a formation from DB. The
/// role name was stored via `role.name()` at persist time. No typetag
/// needed — same pattern as `AutonomyLevel::parse()`.
pub fn from_name(name: &str, capabilities: &[CapabilityDecl]) -> Box<dyn DynamicRoleTrait> {
    match name {
        "Information" => Box::new(InformationAgent::from_original(capabilities)),
        "Support" => Box::new(SupportAgent::new()),
        _ => Box::new(GeneralAgent::new(capabilities.to_vec())),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn apply_to_information_filters_capabilities() {
        let caps: Vec<CapabilityDecl> = vec![
            "github.read_issues".into(),
            "github.create_issue".into(),
        ];
        let role = apply_transformation(&caps, &RoleTransformation::ToInformationAgent);
        assert_eq!(role.name(), "Information");
        assert_eq!(role.capabilities().len(), 1);
    }

    #[test]
    fn apply_to_support() {
        let role = apply_transformation(&[], &RoleTransformation::ToSupportAgent);
        assert_eq!(role.name(), "Support");
    }

    #[test]
    fn apply_reassign_creates_general_with_new_caps() {
        let role = apply_transformation(
            &[],
            &RoleTransformation::ReassignCapabilities(vec![
                "monitoring".into(),
                "logging".into(),
            ]),
        );
        assert_eq!(role.name(), "General");
        assert_eq!(role.capabilities().len(), 2);
    }

    #[test]
    fn from_name_reconstructs_correctly() {
        let caps: Vec<CapabilityDecl> = vec!["github.read_issues".into()];

        let general = from_name("General", &caps);
        assert_eq!(general.name(), "General");
        assert_eq!(general.capabilities().len(), 1);

        let info = from_name("Information", &caps);
        assert_eq!(info.name(), "Information");

        let support = from_name("Support", &caps);
        assert_eq!(support.name(), "Support");

        let unknown = from_name("CustomThing", &caps);
        assert_eq!(unknown.name(), "General"); // unknown falls back to General
    }

    #[test]
    fn cloned_role_retains_behavior() {
        let role = apply_transformation(
            &["github.read_issues".into()],
            &RoleTransformation::ToInformationAgent,
        );
        let cloned = role.clone();
        assert_eq!(role.name(), cloned.name());
        assert_eq!(role.capabilities().len(), cloned.capabilities().len());
    }
}
