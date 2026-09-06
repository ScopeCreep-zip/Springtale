//! Boot-time overrides supplied by the caller (CLI flags on desktop,
//! direct construction when the daemon is booted in-process on mobile).

/// Options that override configuration at boot.
///
/// `Default` reproduces the pre-flag behaviour exactly: bind comes from
/// `[api] bind` in the config file and the passphrase comes from the
/// environment or a TTY prompt.
#[derive(Debug, Clone, Default)]
pub struct BootOptions {
    /// Override for `[api] bind`. `127.0.0.1:0` binds an ephemeral port.
    pub bind: Option<String>,
    /// Read the vault passphrase as one line from stdin.
    pub passphrase_stdin: bool,
}
