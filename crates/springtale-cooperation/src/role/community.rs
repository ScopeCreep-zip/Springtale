//! `CommunityRole` — a declarative, connector-contributable role.
//!
//! Per COOPERATION.md §14.4 footnote: "Connectors can define custom
//! roles implementing the trait." Built-ins (`General`, `Information`,
//! `Support`) live in their own modules and are Rust types. Community
//! connectors — especially WASM ones — can't ship arbitrary Rust impls,
//! so they contribute roles *declaratively*: a name + capability list +
//! action allowlist.
//!
//! The allowlist uses trailing-`*` glob patterns (e.g. `"read_*"`,
//! `"list_*"`, `"observe"`). No regex, no nested globs — kept simple
//! because the matcher runs per action-dispatch in the tick loop and
//! the declarations come from untrusted connector manifests.
//!
//! ## Example
//!
//! ```
//! use springtale_cooperation::role::community::CommunityRole;
//! use springtale_cooperation::role::DynamicRoleTrait;
//! use springtale_cooperation::capability::CapabilityDecl;
//! use springtale_cooperation::cadence::ActionDescriptor;
//!
//! let role = CommunityRole::new(
//!     "ReadOnlyAuditor".into(),
//!     vec![CapabilityDecl::new("github.read_issues")],
//!     vec!["read_*".into(), "list_*".into(), "observe".into()],
//! );
//!
//! let reads = ActionDescriptor { kind: "read_file".into(), target: None, payload_hash: 0 };
//! let writes = ActionDescriptor { kind: "write_file".into(), target: None, payload_hash: 0 };
//! assert!(role.can_execute(&reads));
//! assert!(!role.can_execute(&writes));
//! ```

use crate::cadence::ActionDescriptor;
use crate::capability::CapabilityDecl;

use super::trait_::DynamicRoleTrait;

/// A declaratively-specified role: name + capabilities + allowed-action
/// patterns. Produced by `RoleRegistry::build` when a connector-contributed
/// role is requested.
#[derive(Debug, Clone)]
pub struct CommunityRole {
    name: String,
    capabilities: Vec<CapabilityDecl>,
    /// Patterns tried against `action.kind` in order. A trailing `*`
    /// is a prefix wildcard; everything else is exact match. Empty
    /// list → deny all.
    allowed_actions: Vec<String>,
}

impl CommunityRole {
    pub fn new(
        name: String,
        capabilities: Vec<CapabilityDecl>,
        allowed_actions: Vec<String>,
    ) -> Self {
        Self {
            name,
            capabilities,
            allowed_actions,
        }
    }

    /// Raw access to the allowed-action patterns — exposed for tests
    /// and registry diagnostics.
    pub fn patterns(&self) -> &[String] {
        &self.allowed_actions
    }
}

/// Match one pattern against one action kind. Rules:
/// - exact match returns true;
/// - pattern ending in `*` matches any action kind starting with the
///   part before the `*`;
/// - no other wildcards are supported.
fn pattern_matches(pattern: &str, kind: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('*') {
        kind.starts_with(prefix)
    } else {
        pattern == kind
    }
}

impl DynamicRoleTrait for CommunityRole {
    fn name(&self) -> &str {
        &self.name
    }

    fn can_execute(&self, action: &ActionDescriptor) -> bool {
        self.allowed_actions
            .iter()
            .any(|p| pattern_matches(p, &action.kind))
    }

    fn capabilities(&self) -> &[CapabilityDecl] {
        &self.capabilities
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn action(kind: &str) -> ActionDescriptor {
        ActionDescriptor {
            kind: kind.to_owned(),
            target: None,
            payload_hash: 0,
        }
    }

    #[test]
    fn exact_match_allows() {
        let role = CommunityRole::new("R".into(), vec![], vec!["observe".into()]);
        assert!(role.can_execute(&action("observe")));
        assert!(!role.can_execute(&action("write")));
    }

    #[test]
    fn prefix_wildcard_allows() {
        let role = CommunityRole::new("R".into(), vec![], vec!["read_*".into()]);
        assert!(role.can_execute(&action("read_file")));
        assert!(role.can_execute(&action("read_")));
        assert!(!role.can_execute(&action("write_file")));
    }

    #[test]
    fn multiple_patterns_union() {
        let role = CommunityRole::new(
            "R".into(),
            vec![],
            vec!["read_*".into(), "list_*".into(), "observe".into()],
        );
        assert!(role.can_execute(&action("read_x")));
        assert!(role.can_execute(&action("list_y")));
        assert!(role.can_execute(&action("observe")));
        assert!(!role.can_execute(&action("delete_all")));
    }

    #[test]
    fn empty_allowlist_denies_everything() {
        let role = CommunityRole::new("R".into(), vec![], vec![]);
        assert!(!role.can_execute(&action("anything")));
    }

    #[test]
    fn no_middle_or_leading_wildcards() {
        // The spec-intentional restriction: only trailing `*`. A leading
        // `*` falls back to exact-match semantics and simply won't match
        // anything starting with a non-`*` character.
        let role = CommunityRole::new("R".into(), vec![], vec!["*_read".into()]);
        assert!(!role.can_execute(&action("file_read")));
        assert!(!role.can_execute(&action("anything_read")));
    }

    #[test]
    fn clones_preserve_behavior() {
        let role = CommunityRole::new(
            "R".into(),
            vec![CapabilityDecl::new("cap")],
            vec!["read_*".into()],
        );
        let boxed: Box<dyn DynamicRoleTrait> = Box::new(role);
        let cloned = boxed.clone();
        assert_eq!(boxed.name(), cloned.name());
        assert_eq!(boxed.capabilities().len(), cloned.capabilities().len());
        assert_eq!(
            boxed.can_execute(&action("read_x")),
            cloned.can_execute(&action("read_x"))
        );
    }

    #[test]
    fn capabilities_pass_through() {
        let caps = vec![
            CapabilityDecl::new("github.read_issues"),
            CapabilityDecl::new("github.list_prs"),
        ];
        let role = CommunityRole::new("R".into(), caps.clone(), vec!["read_*".into()]);
        assert_eq!(role.capabilities().len(), 2);
        assert_eq!(role.capabilities()[0].to_string(), "github.read_issues");
    }
}
