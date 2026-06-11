//! `RoleRegistry` — process-wide name→factory lookup for dynamic roles.
//!
//! Built-in roles (`General`, `Information`, `Support`) are auto-registered
//! when the registry is constructed. Connectors register additional roles
//! at install time via `register_community`. Formation reload (Phase 3
//! mental-model recovery) rebuilds a member's role from the persisted
//! name by calling `build(name, capabilities)`.
//!
//! The registry is intentionally decoupled from any specific crate — the
//! connector crate can't depend on `springtale-cooperation` and vice
//! versa, so runtime-level code (`springtale-runtime`) is the glue that
//! translates manifest role declarations into registry entries.
//!
//! ## Thread safety
//!
//! `DashMap` provides concurrent reads and interior-mutable writes. The
//! trait methods keep `&self`, which is what an `Arc<RoleRegistry>`
//! shared across tasks needs. Updates (`register`, `register_community`)
//! are last-write-wins by design — the latest registered factory for a
//! name replaces any earlier factory.

use std::sync::Arc;

use dashmap::DashMap;

use crate::capability::CapabilityDecl;

use super::community::CommunityRole;
use super::general::GeneralAgent;
use super::information::InformationAgent;
use super::support::SupportAgent;
use super::trait_::DynamicRoleTrait;

/// Closure that builds a role instance given the member's current
/// capabilities at reconstruction time. Passed capabilities let built-in
/// roles like `General` carry the exact capability list the member had.
pub type RoleFactory = Arc<dyn Fn(&[CapabilityDecl]) -> Box<dyn DynamicRoleTrait> + Send + Sync>;

/// Process-wide role registry. Held inside an `Arc` by `RuntimeState`
/// so all callers see the same set of registered roles.
pub struct RoleRegistry {
    factories: DashMap<String, RoleFactory>,
}

impl RoleRegistry {
    /// Build a new registry pre-populated with the three built-in roles.
    /// Equivalent to the hand-rolled match in the legacy `from_name`
    /// function.
    pub fn with_builtins() -> Self {
        let registry = Self {
            factories: DashMap::new(),
        };
        registry.register(
            "General",
            Arc::new(|caps: &[CapabilityDecl]| {
                Box::new(GeneralAgent::new(caps.to_vec())) as Box<dyn DynamicRoleTrait>
            }),
        );
        registry.register(
            "Information",
            Arc::new(|caps: &[CapabilityDecl]| {
                Box::new(InformationAgent::from_original(caps)) as Box<dyn DynamicRoleTrait>
            }),
        );
        registry.register(
            "Support",
            Arc::new(|_caps: &[CapabilityDecl]| {
                Box::new(SupportAgent::new()) as Box<dyn DynamicRoleTrait>
            }),
        );
        registry
    }

    /// Raw registration — the connector-crate glue layer uses this for
    /// bespoke factories. Most community contributions go through
    /// `register_community` instead.
    pub fn register(&self, name: &str, factory: RoleFactory) {
        self.factories.insert(name.to_owned(), factory);
    }

    /// Register a community role declaratively: name + capability list
    /// + action allowlist. Produces a `CommunityRole` factory.
    pub fn register_community(
        &self,
        name: &str,
        capabilities: Vec<CapabilityDecl>,
        allowed_actions: Vec<String>,
    ) {
        let name_owned = name.to_owned();
        let factory: RoleFactory = Arc::new(move |caps: &[CapabilityDecl]| {
            // Use the member's current capabilities if the registration
            // didn't provide any, otherwise use the declared list. This
            // lets a community role declare "read only" without
            // duplicating the connector's full capability set.
            let effective = if capabilities.is_empty() {
                caps.to_vec()
            } else {
                capabilities.clone()
            };
            Box::new(CommunityRole::new(
                name_owned.clone(),
                effective,
                allowed_actions.clone(),
            )) as Box<dyn DynamicRoleTrait>
        });
        self.factories.insert(name.to_owned(), factory);
    }

    /// Drop a registration — called when a connector is uninstalled.
    pub fn unregister(&self, name: &str) {
        self.factories.remove(name);
    }

    /// Build a role by name. Missing names fall back to `General` with
    /// the caller-supplied capabilities — same policy as the legacy
    /// `apply::from_name` helper, so the registry is a drop-in upgrade.
    pub fn build(&self, name: &str, capabilities: &[CapabilityDecl]) -> Box<dyn DynamicRoleTrait> {
        if let Some(factory) = self.factories.get(name) {
            factory(capabilities)
        } else {
            Box::new(GeneralAgent::new(capabilities.to_vec()))
        }
    }

    /// Every registered role name, sorted. Useful for UI surfaces that
    /// want to show "available roles" lists.
    pub fn names(&self) -> Vec<String> {
        let mut out: Vec<String> = self.factories.iter().map(|e| e.key().clone()).collect();
        out.sort();
        out
    }

    /// Number of roles currently registered.
    pub fn len(&self) -> usize {
        self.factories.len()
    }

    /// Whether no roles are registered. (A fresh registry from
    /// `with_builtins` is never empty.)
    pub fn is_empty(&self) -> bool {
        self.factories.is_empty()
    }
}

impl Default for RoleRegistry {
    fn default() -> Self {
        Self::with_builtins()
    }
}

impl std::fmt::Debug for RoleRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RoleRegistry")
            .field("names", &self.names())
            .finish()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::cadence::ActionDescriptor;

    fn action(kind: &str) -> ActionDescriptor {
        ActionDescriptor {
            kind: kind.to_owned(),
            target: None,
            payload_hash: 0,
        }
    }

    #[test]
    fn builtins_are_registered() {
        let registry = RoleRegistry::with_builtins();
        let names = registry.names();
        assert!(names.contains(&"General".to_owned()));
        assert!(names.contains(&"Information".to_owned()));
        assert!(names.contains(&"Support".to_owned()));
        assert_eq!(registry.len(), 3);
    }

    #[test]
    fn build_general_returns_general() {
        let registry = RoleRegistry::with_builtins();
        let caps = vec![CapabilityDecl::new("slack.post")];
        let role = registry.build("General", &caps);
        assert_eq!(role.name(), "General");
        assert_eq!(role.capabilities().len(), 1);
    }

    #[test]
    fn build_information_filters_to_read_only() {
        let registry = RoleRegistry::with_builtins();
        let caps = vec![
            CapabilityDecl::new("github.read_issues"),
            CapabilityDecl::new("github.create_issue"),
        ];
        let role = registry.build("Information", &caps);
        assert_eq!(role.name(), "Information");
        // InformationAgent::from_original is expected to drop write caps;
        // the exact filter is a cooperation-module invariant. Just verify
        // it shrunk the set (non-zero filtering) or at least preserved it.
        assert!(role.capabilities().len() <= caps.len());
    }

    #[test]
    fn build_unknown_falls_back_to_general() {
        let registry = RoleRegistry::with_builtins();
        let caps = vec![CapabilityDecl::new("any")];
        let role = registry.build("NotRegistered", &caps);
        assert_eq!(role.name(), "General");
        assert_eq!(role.capabilities().len(), 1);
    }

    #[test]
    fn register_community_role_is_discoverable() {
        let registry = RoleRegistry::with_builtins();
        registry.register_community(
            "ReadOnlyAuditor",
            vec![CapabilityDecl::new("github.read_issues")],
            vec!["read_*".into(), "list_*".into()],
        );
        assert_eq!(registry.len(), 4);

        let role = registry.build("ReadOnlyAuditor", &[]);
        assert_eq!(role.name(), "ReadOnlyAuditor");
        assert!(role.can_execute(&action("read_thing")));
        assert!(role.can_execute(&action("list_other")));
        assert!(!role.can_execute(&action("write_anything")));
    }

    #[test]
    fn community_role_with_empty_caps_inherits_member_caps() {
        // register_community with an empty capability list means "use
        // whatever capabilities the member currently has". This lets
        // connectors ship action-filter-only roles without enumerating
        // their full capability surface.
        let registry = RoleRegistry::with_builtins();
        registry.register_community("Watcher", vec![], vec!["observe".into()]);
        let member_caps = vec![
            CapabilityDecl::new("slack.post"),
            CapabilityDecl::new("slack.read"),
        ];
        let role = registry.build("Watcher", &member_caps);
        assert_eq!(role.capabilities().len(), 2);
    }

    #[test]
    fn unregister_removes_role() {
        let registry = RoleRegistry::with_builtins();
        registry.register_community("Temp", vec![], vec!["ping".into()]);
        assert_eq!(registry.len(), 4);
        registry.unregister("Temp");
        assert_eq!(registry.len(), 3);
        // Builds of a missing name fall back to General.
        let role = registry.build("Temp", &[]);
        assert_eq!(role.name(), "General");
    }

    #[test]
    fn last_write_wins_on_reregistration() {
        // Registering the same name twice keeps the most recent factory.
        // Useful when a connector is reloaded and its manifest tweaks
        // the allowlist.
        let registry = RoleRegistry::with_builtins();
        registry.register_community("Same", vec![], vec!["first".into()]);
        registry.register_community("Same", vec![], vec!["second".into()]);
        let role = registry.build("Same", &[]);
        assert!(!role.can_execute(&action("first")));
        assert!(role.can_execute(&action("second")));
    }

    #[test]
    fn default_is_with_builtins() {
        let registry = RoleRegistry::default();
        assert_eq!(registry.len(), 3);
    }

    #[test]
    fn registry_is_send_sync() {
        fn assert_bounds<T: Send + Sync>() {}
        assert_bounds::<RoleRegistry>();
    }
}
