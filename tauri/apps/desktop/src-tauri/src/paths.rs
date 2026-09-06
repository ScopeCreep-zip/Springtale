//! XDG-compliant path resolution for the desktop shell.
//!
//! The desktop is a sidecar client: `springtaled` owns the database and
//! everything else under the data directory. The one file the shell still
//! touches directly is the vault, because the vault overlay has to accept
//! a passphrase and create the identity before the daemon can be started
//! with it. Mirrors `springtale_store::paths` — the daemon and the shell
//! must agree on where `vault.bin` lives.

use std::path::PathBuf;

/// XDG-compliant data directory for Springtale.
///
/// Resolution order:
/// 1. `$XDG_DATA_HOME/springtale`
/// 2. `$HOME/.local/share/springtale`
/// 3. `.springtale` (current directory fallback)
pub fn data_dir() -> PathBuf {
    xdg_data_base().unwrap_or_else(|| PathBuf::from(".springtale"))
}

/// Default path to the encrypted vault file.
pub fn default_vault_path() -> PathBuf {
    data_dir().join("vault.bin")
}

/// Resolve the XDG data base directory, returning `None` if neither
/// `XDG_DATA_HOME` nor `HOME` is set.
fn xdg_data_base() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .map(|base| base.join("springtale"))
}
