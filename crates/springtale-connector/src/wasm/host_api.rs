use crate::capability::grant::CapabilityChecker;
use crate::error::ConnectorError;
use crate::manifest::types::Capability;

// WASI host functions exposed to WASM guest connectors.
//
// Every host function is gated by the connector's declared capabilities.
// A WASM guest calling `host_network_request` without `NetworkOutbound`
// in its manifest gets a trap, not a network call.
//
// Phase 1a: defines the interface and capability gating logic.
// Actual WASI P2 bindings (wasmtime-wasi WasiCtx integration) will be
// wired when the first WASM connector is built and tested.

/// Validate that a WASM guest is allowed to make an outbound network request.
///
/// Called from the WASI host function that handles network I/O.
/// Returns Ok if the host is in the connector's approved NetworkOutbound list.
pub fn gate_network_outbound(
    checker: &CapabilityChecker,
    connector_name: &str,
    target_host: &str,
) -> Result<(), ConnectorError> {
    let required = Capability::NetworkOutbound {
        host: target_host.to_owned(),
    };
    checker.check(connector_name, &required)
}

/// Validate that a WASM guest is allowed to read from the filesystem.
pub fn gate_filesystem_read(
    checker: &CapabilityChecker,
    connector_name: &str,
    path: &str,
) -> Result<(), ConnectorError> {
    let required = Capability::FilesystemRead {
        path: path.to_owned(),
    };
    checker.check(connector_name, &required)
}

/// Validate that a WASM guest is allowed to write to the filesystem.
pub fn gate_filesystem_write(
    checker: &CapabilityChecker,
    connector_name: &str,
    path: &str,
) -> Result<(), ConnectorError> {
    let required = Capability::FilesystemWrite {
        path: path.to_owned(),
    };
    checker.check(connector_name, &required)
}

/// Validate that a WASM guest is allowed to read a keychain entry.
pub fn gate_keychain_read(
    checker: &CapabilityChecker,
    connector_name: &str,
    key: &str,
) -> Result<(), ConnectorError> {
    let required = Capability::KeychainRead {
        key: key.to_owned(),
    };
    checker.check(connector_name, &required)
}

/// Validate that a WASM guest is allowed to execute shell commands.
pub fn gate_shell_exec(
    checker: &CapabilityChecker,
    connector_name: &str,
) -> Result<(), ConnectorError> {
    checker.check(connector_name, &Capability::ShellExec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::grant::{CapabilityChecker, CapabilityPolicy};

    fn setup_checker(caps: Vec<Capability>) -> CapabilityChecker {
        let mut checker = CapabilityChecker::new();
        checker
            .register("connector-test", &caps, &CapabilityPolicy::AllowAll)
            .ok();
        checker
    }

    #[test]
    fn test_gate_network_approved_host() {
        let checker = setup_checker(vec![Capability::NetworkOutbound {
            host: "api.example.com".into(),
        }]);
        assert!(gate_network_outbound(&checker, "connector-test", "api.example.com").is_ok());
    }

    #[test]
    fn test_gate_network_unapproved_host() {
        let checker = setup_checker(vec![Capability::NetworkOutbound {
            host: "api.example.com".into(),
        }]);
        assert!(gate_network_outbound(&checker, "connector-test", "evil.com").is_err());
    }

    #[test]
    fn test_gate_filesystem_read_approved() {
        let checker = setup_checker(vec![Capability::FilesystemRead {
            path: "/tmp/data".into(),
        }]);
        assert!(gate_filesystem_read(&checker, "connector-test", "/tmp/data").is_ok());
    }

    #[test]
    fn test_gate_filesystem_read_wrong_path() {
        let checker = setup_checker(vec![Capability::FilesystemRead {
            path: "/tmp/data".into(),
        }]);
        assert!(gate_filesystem_read(&checker, "connector-test", "/etc/passwd").is_err());
    }

    #[test]
    fn test_gate_shell_exec_approved() {
        // ShellExec is policy-exempt (Phase-7 Finding A) — even
        // `AllowAll` registration lands it in pending_approval. The
        // WASM host-API gate just checks "is this cap approved?";
        // promotion from pending → approved is the bridge's
        // ApprovalGate's job. Here we simulate a gate-already-fired
        // path by manually calling `approve()` before the gate check.
        let mut checker = setup_checker(vec![Capability::ShellExec]);
        checker.approve("connector-test", &Capability::ShellExec); // ci-hardening-allow: gated-path test fixture
        assert!(gate_shell_exec(&checker, "connector-test").is_ok());
    }

    #[test]
    fn test_gate_shell_exec_not_declared() {
        let checker = setup_checker(vec![Capability::NetworkOutbound {
            host: "api.example.com".into(),
        }]);
        assert!(gate_shell_exec(&checker, "connector-test").is_err());
    }
}
