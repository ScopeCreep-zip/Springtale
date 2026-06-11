//! Glue between `ConnectorManifest::roles` (connector crate) and
//! `RoleRegistry` (cooperation crate).
//!
//! The connector crate cannot depend on cooperation, and cooperation
//! cannot depend on connector — so the translation lives here, one
//! level up, where both are in scope.
//!
//! Behavior:
//! - Each `RoleDecl` in the manifest becomes a `register_community`
//!   call with the declared capabilities + action allowlist.
//! - Uninstalling a connector calls `unregister_manifest_roles` with
//!   the same manifest so the factories are dropped.
//! - Builds over the same name are last-write-wins per the registry
//!   contract, so reinstall safely replaces stale factories.

use std::sync::Arc;

use springtale_connector::manifest::types::ConnectorManifest;
use springtale_cooperation::capability::CapabilityDecl;
use springtale_cooperation::role::RoleRegistry;

/// Translate `manifest.roles` into `RoleRegistry::register_community`
/// calls. Safe to call repeatedly; re-registration replaces the prior
/// factory atomically.
pub fn register_manifest_roles(registry: &Arc<RoleRegistry>, manifest: &ConnectorManifest) {
    for decl in &manifest.roles {
        let caps: Vec<CapabilityDecl> = decl
            .capabilities
            .iter()
            .map(|c| CapabilityDecl::new(c.as_str()))
            .collect();
        registry.register_community(&decl.name, caps, decl.allowed_actions.clone());
        tracing::info!(
            connector = %manifest.name,
            role = %decl.name,
            actions = decl.allowed_actions.len(),
            "registered community role"
        );
    }
}

/// Drop any roles this manifest contributed — called when the connector
/// is uninstalled so formation reload can't resurrect a role whose
/// implementation is gone.
pub fn unregister_manifest_roles(registry: &Arc<RoleRegistry>, manifest: &ConnectorManifest) {
    for decl in &manifest.roles {
        registry.unregister(&decl.name);
        tracing::info!(
            connector = %manifest.name,
            role = %decl.name,
            "unregistered community role"
        );
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use springtale_connector::manifest::SignatureAlgorithm;
    use springtale_connector::manifest::types::{Capability, ConnectorManifest, RoleDecl};
    use springtale_cooperation::cadence::ActionDescriptor;

    fn sample_manifest() -> ConnectorManifest {
        ConnectorManifest {
            name: "connector-sample".into(),
            version: "0.1.0".into(),
            author: "test".into(),
            description: "sample".into(),
            capabilities: vec![Capability::NetworkOutbound {
                host: "api.example.com".into(),
            }],
            triggers: vec![],
            actions: vec![],
            data_disclosure: vec![],
            roles: vec![
                RoleDecl {
                    name: "Watcher".into(),
                    description: "observe-only".into(),
                    capabilities: vec!["read".into()],
                    allowed_actions: vec!["read_*".into(), "list_*".into()],
                },
                RoleDecl {
                    name: "Writer".into(),
                    description: "writes".into(),
                    capabilities: vec!["write".into()],
                    allowed_actions: vec!["write_*".into()],
                },
            ],
            wasm_hash: None,
            signature_alg: SignatureAlgorithm::default(),
            signature: None,
        }
    }

    fn action(kind: &str) -> ActionDescriptor {
        ActionDescriptor {
            kind: kind.to_owned(),
            target: None,
            payload_hash: 0,
        }
    }

    #[test]
    fn register_manifest_roles_adds_all_declared_roles() {
        let registry = Arc::new(RoleRegistry::with_builtins());
        register_manifest_roles(&registry, &sample_manifest());
        let names = registry.names();
        assert!(names.contains(&"Watcher".to_owned()));
        assert!(names.contains(&"Writer".to_owned()));

        // Verify action gating works end-to-end through the registry.
        let watcher = registry.build("Watcher", &[]);
        assert!(watcher.can_execute(&action("read_file")));
        assert!(!watcher.can_execute(&action("write_file")));
        let writer = registry.build("Writer", &[]);
        assert!(writer.can_execute(&action("write_thing")));
        assert!(!writer.can_execute(&action("read_thing")));
    }

    #[test]
    fn unregister_manifest_roles_drops_contributed_roles() {
        let registry = Arc::new(RoleRegistry::with_builtins());
        let manifest = sample_manifest();
        register_manifest_roles(&registry, &manifest);
        assert_eq!(registry.len(), 5);
        unregister_manifest_roles(&registry, &manifest);
        assert_eq!(registry.len(), 3); // back to built-ins only
    }

    #[test]
    fn register_is_idempotent_across_reinstalls() {
        let registry = Arc::new(RoleRegistry::with_builtins());
        let manifest = sample_manifest();
        register_manifest_roles(&registry, &manifest);
        register_manifest_roles(&registry, &manifest);
        // Second call replaces (not duplicates) the previous entries.
        assert_eq!(registry.len(), 5);
    }
}
