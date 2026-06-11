//! DynamicRoleTrait — the composable decision policy per agent.
//!
//! Per COOPERATION.md §14.4:
//! ```text
//! #[typetag::serde(tag = "role")]
//! pub trait DynamicRole: Send + Sync {
//!     fn name(&self) -> &'static str;
//!     fn can_execute(&self, action: &ActionDescriptor) -> bool;
//!     fn execute(&self, ...) -> Result<ExecutionResult, RoleError>;
//! }
//! ```
//!
//! We skip typetag (requires Bevy-style inventory linker gotcha) and
//! instead persist role.name() as a string, reconstruct via from_name().
//! DynClone supertrait (from dyn-clone crate) makes Box<dyn> clonable.

use dyn_clone::DynClone;

use crate::cadence::ActionDescriptor;
use crate::capability::CapabilityDecl;

/// The core role trait — each implementation encapsulates its own
/// decision policy. The agent loop calls `can_execute` before every
/// action dispatch.
///
/// Per Spring RTS `AllowedCommand`: if `canAttack` is false, the attack
/// command is rejected at the unit level, not at the global level.
/// Per OpenAI Swarm: when a handoff fires, the active agent's entire
/// instruction set swaps. That's what `apply_transformation` does —
/// replaces the `Box<dyn DynamicRoleTrait>` on a FormationMember.
pub trait DynamicRoleTrait: DynClone + Send + Sync + std::fmt::Debug {
    /// Display name for UI + DB persistence. Used by `from_name()` to
    /// reconstruct the role on formation restart.
    fn name(&self) -> &str;

    /// Whether this role allows the given action to proceed.
    /// Per Spring: `canAttack`, `canMove`, `canReclaim` as boolean flags.
    /// Per Siege dead→cameras: InformationAgent only allows "read"/"observe".
    fn can_execute(&self, action: &ActionDescriptor) -> bool;

    /// What capabilities this role exposes to the routing layer.
    /// General: all connector caps. Information: read-only subset.
    fn capabilities(&self) -> &[CapabilityDecl];
}

dyn_clone::clone_trait_object!(DynamicRoleTrait);

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[derive(Debug, Clone)]
    struct TestRole;

    impl DynamicRoleTrait for TestRole {
        fn name(&self) -> &str {
            "Test"
        }
        fn can_execute(&self, _action: &ActionDescriptor) -> bool {
            true
        }
        fn capabilities(&self) -> &[CapabilityDecl] {
            &[]
        }
    }

    #[test]
    fn box_dyn_is_clonable() {
        let role: Box<dyn DynamicRoleTrait> = Box::new(TestRole);
        let cloned = role.clone();
        assert_eq!(role.name(), cloned.name());
    }

    #[test]
    fn trait_object_is_debuggable() {
        let role: Box<dyn DynamicRoleTrait> = Box::new(TestRole);
        let debug = format!("{role:?}");
        assert!(debug.contains("TestRole"));
    }
}
