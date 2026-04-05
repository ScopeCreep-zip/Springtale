use springtale_connector::manifest::types::Capability;

use crate::error::SentinelError;

/// Checks a connector's declared capabilities for dangerous combinations.
///
/// Toxic pairs are capability combinations that enable attack patterns
/// like credential exfiltration or unauthorized data access. These are
/// blocked at connector install time, not at runtime.
///
/// The 5 blocked pairs (from ARCHITECTURE.md §15.3):
/// 1. KeychainRead + NetworkOutbound (to different host) — credential exfil
/// 2. FilesystemRead + NetworkOutbound — data exfil
/// 3. ShellExec + NetworkOutbound — reverse shell
/// 4. FilesystemWrite + ShellExec — write + execute payload
/// 5. KeychainRead + ShellExec — credential use in shell
pub fn check_toxic_pairs(capabilities: &[Capability]) -> Result<(), SentinelError> {
    let has_keychain = capabilities
        .iter()
        .any(|c| matches!(c, Capability::KeychainRead { .. }));
    let has_fs_read = capabilities
        .iter()
        .any(|c| matches!(c, Capability::FilesystemRead { .. }));
    let has_fs_write = capabilities
        .iter()
        .any(|c| matches!(c, Capability::FilesystemWrite { .. }));
    let has_shell = capabilities
        .iter()
        .any(|c| matches!(c, Capability::ShellExec));
    let has_network = capabilities
        .iter()
        .any(|c| matches!(c, Capability::NetworkOutbound { .. }));

    if has_keychain && has_network {
        return Err(SentinelError::ToxicPair(
            "KeychainRead + NetworkOutbound: potential credential exfiltration".into(),
        ));
    }

    if has_fs_read && has_network {
        return Err(SentinelError::ToxicPair(
            "FilesystemRead + NetworkOutbound: potential data exfiltration".into(),
        ));
    }

    if has_shell && has_network {
        return Err(SentinelError::ToxicPair(
            "ShellExec + NetworkOutbound: potential reverse shell".into(),
        ));
    }

    if has_fs_write && has_shell {
        return Err(SentinelError::ToxicPair(
            "FilesystemWrite + ShellExec: potential payload write + execute".into(),
        ));
    }

    if has_keychain && has_shell {
        return Err(SentinelError::ToxicPair(
            "KeychainRead + ShellExec: potential credential use in shell".into(),
        ));
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_combination_passes() {
        let caps = vec![Capability::NetworkOutbound {
            host: "api.example.com".into(),
        }];
        assert!(check_toxic_pairs(&caps).is_ok());
    }

    #[test]
    fn test_keychain_plus_network_blocked() {
        let caps = vec![
            Capability::KeychainRead {
                key: "token".into(),
            },
            Capability::NetworkOutbound {
                host: "evil.com".into(),
            },
        ];
        assert!(matches!(
            check_toxic_pairs(&caps),
            Err(SentinelError::ToxicPair(_))
        ));
    }

    #[test]
    fn test_fs_read_plus_network_blocked() {
        let caps = vec![
            Capability::FilesystemRead {
                path: "/etc".into(),
            },
            Capability::NetworkOutbound {
                host: "exfil.com".into(),
            },
        ];
        assert!(check_toxic_pairs(&caps).is_err());
    }

    #[test]
    fn test_shell_plus_network_blocked() {
        let caps = vec![
            Capability::ShellExec,
            Capability::NetworkOutbound {
                host: "c2.evil.com".into(),
            },
        ];
        assert!(check_toxic_pairs(&caps).is_err());
    }

    #[test]
    fn test_fs_write_plus_shell_blocked() {
        let caps = vec![
            Capability::FilesystemWrite {
                path: "/tmp".into(),
            },
            Capability::ShellExec,
        ];
        assert!(check_toxic_pairs(&caps).is_err());
    }

    #[test]
    fn test_keychain_plus_shell_blocked() {
        let caps = vec![
            Capability::KeychainRead {
                key: "token".into(),
            },
            Capability::ShellExec,
        ];
        assert!(check_toxic_pairs(&caps).is_err());
    }

    #[test]
    fn test_single_capability_passes() {
        assert!(check_toxic_pairs(&[Capability::ShellExec]).is_ok());
        assert!(check_toxic_pairs(&[Capability::KeychainRead { key: "k".into() }]).is_ok());
    }

    #[test]
    fn test_empty_passes() {
        assert!(check_toxic_pairs(&[]).is_ok());
    }
}
