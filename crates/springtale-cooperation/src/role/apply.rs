//! Role transformation application — swaps the Box<dyn DynamicRoleTrait>.
//!
//! Per OpenAI Swarm: handoff = swap active agent entirely.
//! Per RTS: unit role changes, old role drops, new role takes over.
//! No fallback to old role — full forward replacement.
//!
//! Two entry points:
//! - `apply_transformation` — direct variant-to-role mapping for the
//!   built-in `RoleTransformation` enum. Bypasses the registry; kept
//!   for call sites that don't have one in scope (legacy tests, low-
//!   level internal transitions).
//! - `apply_transformation_via_registry` — preferred path. Routes
//!   `ReassignCapabilities` and `ToInformationAgent`/`ToSupportAgent`
//!   through `RoleRegistry::build`, which picks up community roles
//!   contributed by connector manifests (§14.4 / Phase 21).

use crate::capability::CapabilityDecl;
use crate::transformation::RoleTransformation;

use super::general::GeneralAgent;
use super::information::InformationAgent;
use super::registry::RoleRegistry;
use super::support::SupportAgent;
use super::trait_::DynamicRoleTrait;

/// Apply a role transformation without consulting the registry —
/// hardcoded built-in factories only.
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

/// Registry-aware variant — resolves built-in transformation names
/// through the shared `RoleRegistry` so community roles registered
/// from connector manifests can also participate. The target role
/// name is `"Information"` / `"Support"` / `"General"` for the three
/// built-in variants; when the registry has an identically-named
/// community role it wins (last-write-wins semantics match the
/// registry contract). For `ReassignCapabilities` the call always
/// rebuilds `General` with the new caps (by design — reassignment is
/// a first-party policy, not a community role transition).
pub fn apply_transformation_via_registry(
    registry: &RoleRegistry,
    current_capabilities: &[CapabilityDecl],
    transformation: &RoleTransformation,
) -> Box<dyn DynamicRoleTrait> {
    match transformation {
        RoleTransformation::ToInformationAgent => {
            registry.build("Information", current_capabilities)
        }
        RoleTransformation::ToSupportAgent => registry.build("Support", &[]),
        RoleTransformation::ReassignCapabilities(new_caps) => registry.build("General", new_caps),
    }
}

/// Reconstruct a role from its persisted name string — hardcoded
/// built-in lookup.
///
/// Called by `spawn_formation` when loading a formation from DB. The
/// role name was stored via `role.name()` at persist time.
///
/// Prefer [`from_name_via_registry`] when a registry is in scope so
/// community roles resolve correctly.
pub fn from_name(name: &str, capabilities: &[CapabilityDecl]) -> Box<dyn DynamicRoleTrait> {
    match name {
        "Information" => Box::new(InformationAgent::from_original(capabilities)),
        "Support" => Box::new(SupportAgent::new()),
        _ => Box::new(GeneralAgent::new(capabilities.to_vec())),
    }
}

/// Registry-aware role reconstruction. Missing names fall back to
/// `General` via the registry's own fallback — same behavior as the
/// legacy `from_name`, but community roles are resolvable.
pub fn from_name_via_registry(
    registry: &RoleRegistry,
    name: &str,
    capabilities: &[CapabilityDecl],
) -> Box<dyn DynamicRoleTrait> {
    registry.build(name, capabilities)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn apply_to_information_filters_capabilities() {
        let caps: Vec<CapabilityDecl> =
            vec!["github.read_issues".into(), "github.create_issue".into()];
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
            &RoleTransformation::ReassignCapabilities(vec!["monitoring".into(), "logging".into()]),
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
