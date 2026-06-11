use std::collections::HashSet;

use crate::error::ConnectorError;
use crate::manifest::types::Capability;
use crate::tier::WasmTier;

/// A granted capability for a specific connector.
#[derive(Debug, Clone)]
pub struct CapabilityGrant {
    /// The connector this grant applies to.
    pub connector_name: String,

    /// Capabilities that have been approved.
    pub approved: HashSet<String>,

    /// Capabilities that are pending user approval.
    pub pending_approval: Vec<Capability>,

    /// Capabilities that were denied.
    pub denied: Vec<Capability>,
}

/// User's policy for capability approval.
#[derive(Debug, Clone, Default)]
pub enum CapabilityPolicy {
    /// Auto-approve all capabilities (not recommended).
    AllowAll,

    /// Auto-deny all capabilities (maximum restriction).
    DenyAll,

    /// Approve only these specific capabilities. Everything else denied.
    AllowList(HashSet<String>),

    /// Prompt user for each new capability (default).
    #[default]
    Interactive,
}

/// Runtime capability checker.
///
/// Created when a connector is installed. Called BEFORE every `execute()`.
/// The connector cannot bypass this — it sits in the dispatch layer.
///
/// Derives `Clone` so the dispatch layer can clone the checker and drop the
/// registry lock before executing connector actions (avoids holding the lock
/// across potentially long network calls).
///
/// The `tier` field carries the active momentum tier for this specific
/// invocation (COOPERATION.md §16). The WASM sandbox uses it to select
/// which per-tier `InstancePre` to instantiate; native connectors can
/// inspect it too if they want to adjust behavior under Cold vs Hot.
/// Per-invocation scoping (not per-host) is what lets a single WASM
/// connector be shared across formations that sit at different tiers.
#[derive(Clone)]
pub struct CapabilityChecker {
    grants: std::collections::HashMap<String, CapabilityGrant>,
    tier: WasmTier,
}

impl CapabilityChecker {
    /// New checker with the permissive default tier (Warming).
    ///
    /// Non-formation connector calls (chat-command handlers, CLI, one-shot
    /// API actions) aren't bound to any momentum state, so they start
    /// with the same host primitives they had before Phase 16 landed —
    /// i.e. HTTP is allowed. Formation-scoped executions override via
    /// `with_tier(...)` (driven by `CapabilityBridge` from the calling
    /// formation's `MomentumTier`). A caller that wants the strictest
    /// gate builds `CapabilityChecker::new().with_tier(WasmTier::Cold)`.
    pub fn new() -> Self {
        Self {
            grants: std::collections::HashMap::new(),
            tier: WasmTier::Warming,
        }
    }

    /// Set the momentum tier for this invocation. Consumes and returns
    /// `self` so callers can chain `checker.clone().with_tier(t)` right
    /// before `execute_checked`.
    #[must_use]
    pub fn with_tier(mut self, tier: WasmTier) -> Self {
        self.tier = tier;
        self
    }

    /// Momentum tier bound to this checker — consulted by
    /// `WasmConnectorHost::execute_checked` to pick the right tier
    /// cache entry.
    pub fn tier(&self) -> WasmTier {
        self.tier
    }

    /// Register a connector's approved capabilities.
    pub fn register(
        &mut self,
        connector_name: &str,
        declared: &[Capability],
        policy: &CapabilityPolicy,
    ) -> Result<CapabilityGrant, ConnectorError> {
        let mut approved = HashSet::new();
        let mut pending = Vec::new();
        let mut denied = Vec::new();

        for cap in declared {
            let cap_str = cap.to_string();
            // ShellExec is policy-exempt: NO policy (including AllowAll
            // and AllowList) may auto-approve it. The blocking
            // ApprovalGate at the runtime layer is the only path from
            // pending → approved. This makes the rule architectural
            // rather than policy-conditional and closes the OpenClaw
            // CVE-2026-25253 1-click-RCE class — see
            // `docs/security/RISK-REGISTER.md` R-005 and Phase-7
            // audit Finding A in `~/.claude/plans/mighty-honking-pinwheel.md`.
            if matches!(cap, Capability::ShellExec) {
                if matches!(policy, CapabilityPolicy::DenyAll) {
                    denied.push(cap.clone());
                } else {
                    pending.push(cap.clone());
                }
                continue;
            }
            match policy {
                CapabilityPolicy::AllowAll => {
                    approved.insert(cap_str);
                }
                CapabilityPolicy::DenyAll => {
                    denied.push(cap.clone());
                }
                CapabilityPolicy::AllowList(allowed) => {
                    if allowed.contains(&cap_str) {
                        approved.insert(cap_str);
                    } else {
                        denied.push(cap.clone());
                    }
                }
                CapabilityPolicy::Interactive => {
                    // Non-dangerous capabilities auto-approved in interactive mode.
                    // (ShellExec already handled by the policy-exempt branch above.)
                    approved.insert(cap_str);
                }
            }
        }

        let grant = CapabilityGrant {
            connector_name: connector_name.to_owned(),
            approved,
            pending_approval: pending,
            denied,
        };

        self.grants.insert(connector_name.to_owned(), grant.clone());
        Ok(grant)
    }

    /// Approve a pending capability (called after user confirms).
    pub fn approve(&mut self, connector_name: &str, capability: &Capability) -> bool {
        if let Some(grant) = self.grants.get_mut(connector_name) {
            let cap_str = capability.to_string();
            grant.pending_approval.retain(|c| c.to_string() != cap_str);
            grant.approved.insert(cap_str);
            true
        } else {
            false
        }
    }

    /// Check if a connector has a specific capability at runtime.
    ///
    /// This is called BEFORE every `execute()` in the dispatch layer.
    pub fn check(&self, connector_name: &str, required: &Capability) -> Result<(), ConnectorError> {
        let grant = self
            .grants
            .get(connector_name)
            .ok_or_else(|| ConnectorError::NotFound(connector_name.to_owned()))?;

        let cap_str = required.to_string();

        if grant.approved.contains(&cap_str) {
            Ok(())
        } else if grant
            .pending_approval
            .iter()
            .any(|c| c.to_string() == cap_str)
        {
            Err(ConnectorError::RequiresApproval(cap_str))
        } else {
            Err(ConnectorError::CapabilityDenied(cap_str))
        }
    }
}

impl Default for CapabilityChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_allow_all_policy() {
        let mut checker = CapabilityChecker::new();
        let caps = vec![
            Capability::NetworkOutbound {
                host: "api.kick.com".into(),
            },
            Capability::ShellExec,
        ];

        let grant = checker
            .register("connector-test", &caps, &CapabilityPolicy::AllowAll)
            .unwrap();

        // AllowAll approves NetworkOutbound but ShellExec is
        // policy-exempt — it ALWAYS routes through the blocking
        // ApprovalGate, even under AllowAll. See the dedicated
        // `test_shell_exec_always_pending_under_allow_all` regression.
        assert_eq!(grant.approved.len(), 1);
        assert!(
            grant
                .approved
                .iter()
                .any(|c| c.starts_with("NetworkOutbound"))
        );
        assert_eq!(grant.pending_approval, vec![Capability::ShellExec]);
        assert!(grant.denied.is_empty());
    }

    #[test]
    fn test_deny_all_policy() {
        let mut checker = CapabilityChecker::new();
        let caps = vec![Capability::NetworkOutbound {
            host: "api.kick.com".into(),
        }];

        let grant = checker
            .register("connector-test", &caps, &CapabilityPolicy::DenyAll)
            .unwrap();

        assert!(grant.approved.is_empty());
        assert_eq!(grant.denied.len(), 1);
    }

    #[test]
    fn test_interactive_auto_approves_network() {
        let mut checker = CapabilityChecker::new();
        let caps = vec![Capability::NetworkOutbound {
            host: "api.kick.com".into(),
        }];

        let grant = checker
            .register("connector-test", &caps, &CapabilityPolicy::Interactive)
            .unwrap();

        assert_eq!(grant.approved.len(), 1);
        assert!(grant.pending_approval.is_empty());
    }

    #[test]
    fn test_interactive_holds_shell_exec() {
        let mut checker = CapabilityChecker::new();
        let caps = vec![Capability::ShellExec];

        let grant = checker
            .register("connector-test", &caps, &CapabilityPolicy::Interactive)
            .unwrap();

        assert!(grant.approved.is_empty());
        assert_eq!(grant.pending_approval.len(), 1);
    }

    #[test]
    fn test_check_approved_passes() {
        let mut checker = CapabilityChecker::new();
        let caps = vec![Capability::NetworkOutbound {
            host: "api.kick.com".into(),
        }];
        checker
            .register("connector-test", &caps, &CapabilityPolicy::AllowAll)
            .unwrap();

        let result = checker.check(
            "connector-test",
            &Capability::NetworkOutbound {
                host: "api.kick.com".into(),
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_denied_fails() {
        let mut checker = CapabilityChecker::new();
        let caps = vec![Capability::NetworkOutbound {
            host: "api.kick.com".into(),
        }];
        checker
            .register("connector-test", &caps, &CapabilityPolicy::DenyAll)
            .unwrap();

        let result = checker.check(
            "connector-test",
            &Capability::NetworkOutbound {
                host: "api.kick.com".into(),
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_check_undeclared_capability_fails() {
        let mut checker = CapabilityChecker::new();
        let caps = vec![Capability::NetworkOutbound {
            host: "api.kick.com".into(),
        }];
        checker
            .register("connector-test", &caps, &CapabilityPolicy::AllowAll)
            .unwrap();

        // Requesting a capability that was never declared
        let result = checker.check(
            "connector-test",
            &Capability::NetworkOutbound {
                host: "evil.com".into(),
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_approve_pending_capability() {
        let mut checker = CapabilityChecker::new();
        let caps = vec![Capability::ShellExec];
        checker
            .register("connector-test", &caps, &CapabilityPolicy::Interactive)
            .unwrap();

        // ShellExec is pending
        assert!(
            checker
                .check("connector-test", &Capability::ShellExec)
                .is_err()
        );

        // User approves
        assert!(checker.approve("connector-test", &Capability::ShellExec));

        // Now it passes
        assert!(
            checker
                .check("connector-test", &Capability::ShellExec)
                .is_ok()
        );
    }

    /// Phase-7 audit Finding A regression. `ShellExec` is the
    /// OpenClaw CVE-2026-25253 1-click-RCE class — under NO policy
    /// (not even the documented "not recommended" `AllowAll`) may it
    /// auto-approve. The blocking `ApprovalGate` at the runtime
    /// layer is the only path from pending → approved.
    #[test]
    fn test_shell_exec_always_pending_under_allow_all() {
        let mut checker = CapabilityChecker::new();
        let grant = checker
            .register(
                "connector-test",
                &[Capability::ShellExec],
                &CapabilityPolicy::AllowAll,
            )
            .unwrap();
        // Must land in pending, NOT approved — even under AllowAll.
        assert!(
            !grant.approved.contains(&Capability::ShellExec.to_string()),
            "ShellExec must NOT be auto-approved under AllowAll"
        );
        assert!(
            grant.pending_approval.contains(&Capability::ShellExec),
            "ShellExec must land in pending_approval"
        );
        // And the check rejects until ApprovalGate moves it.
        assert!(
            checker
                .check("connector-test", &Capability::ShellExec)
                .is_err()
        );
    }

    #[test]
    fn test_shell_exec_always_pending_under_allow_list() {
        // Even if the user explicitly allow-lists "ShellExec" by
        // string, the policy carve-out routes through pending. The
        // user signalled intent to allow, but the blocking gate
        // still has to land the actual approval per-invocation.
        let allowed: HashSet<String> = [Capability::ShellExec.to_string()].into_iter().collect();
        let mut checker = CapabilityChecker::new();
        let grant = checker
            .register(
                "connector-test",
                &[Capability::ShellExec],
                &CapabilityPolicy::AllowList(allowed),
            )
            .unwrap();
        assert!(
            !grant.approved.contains(&Capability::ShellExec.to_string()),
            "ShellExec must NOT be auto-approved even when explicitly allow-listed"
        );
        assert!(grant.pending_approval.contains(&Capability::ShellExec));
    }

    #[test]
    fn test_shell_exec_denied_under_deny_all() {
        // DenyAll keeps ShellExec denied — the policy carve-out is
        // only "always go through the gate", not "always pending"
        // when the user has explicitly denied everything.
        let mut checker = CapabilityChecker::new();
        let grant = checker
            .register(
                "connector-test",
                &[Capability::ShellExec],
                &CapabilityPolicy::DenyAll,
            )
            .unwrap();
        assert!(grant.denied.contains(&Capability::ShellExec));
        assert!(!grant.pending_approval.contains(&Capability::ShellExec));
        assert!(!grant.approved.contains(&Capability::ShellExec.to_string()));
    }
}
