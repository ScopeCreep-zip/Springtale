//! XDG-compliant path resolution for Springtale data files.
//!
//! Shared across daemon and CLI to avoid duplicating platform-specific
//! directory logic. All paths resolve under `$XDG_DATA_HOME/springtale`
//! (or `$HOME/.local/share/springtale`), falling back to `.springtale/`
//! if neither variable is set.

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

/// Default path to the SQLite database file.
pub fn default_db_path() -> PathBuf {
    data_dir().join("springtale.db")
}

/// Default path to the encrypted vault file.
pub fn default_vault_path() -> PathBuf {
    data_dir().join("vault.bin")
}

/// Default path to the Unix domain socket.
pub fn default_socket_path() -> PathBuf {
    data_dir().join("springtale.sock")
}

/// Resolve the XDG data base directory, returning `None` if neither
/// `XDG_DATA_HOME` nor `HOME` is set.
fn xdg_data_base() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
        })
        .map(|base| base.join("springtale"))
}
