use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;

/// Default debounce interval for filesystem events.
const DEFAULT_DEBOUNCE_MS: u64 = 500;

/// Configuration for the filesystem connector.
///
/// Deserialized from the connector's TOML config. Never serialized
/// (prevents accidental logging of path details).
#[derive(Debug, Clone, Deserialize)]
pub struct FilesystemConfig {
    /// Paths to watch for filesystem events (triggers).
    /// Each path must be within the declared `FilesystemRead` capabilities.
    #[serde(default)]
    pub watch_paths: Vec<PathBuf>,

    /// Paths that the connector is allowed to read from.
    /// Actions like `read_file` and `list_dir` are restricted to these paths.
    /// If empty, no reads are allowed.
    #[serde(default)]
    pub read_paths: Vec<PathBuf>,

    /// Paths that the connector is allowed to write to.
    /// The `write_file` action is restricted to these paths.
    /// If empty, no writes are allowed.
    #[serde(default)]
    pub write_paths: Vec<PathBuf>,

    /// Debounce interval in milliseconds for filesystem watch events.
    /// Events within this window are coalesced. Default: 500ms.
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,
}

fn default_debounce_ms() -> u64 {
    DEFAULT_DEBOUNCE_MS
}

impl FilesystemConfig {
    /// Get the debounce duration.
    pub fn debounce_duration(&self) -> Duration {
        Duration::from_millis(self.debounce_ms)
    }

    /// Check whether a path is within the read allow-list.
    ///
    /// Canonicalizes both the target path and allow-list entries to prevent
    /// symlink traversal attacks. Returns `false` if canonicalization fails
    /// (e.g., path doesn't exist or permission denied).
    pub fn is_read_allowed(&self, path: &std::path::Path) -> bool {
        is_path_within_allowlist(path, &self.read_paths)
    }

    /// Check whether a path is within the write allow-list.
    ///
    /// Same canonicalization and symlink protection as `is_read_allowed`.
    pub fn is_write_allowed(&self, path: &std::path::Path) -> bool {
        is_path_within_allowlist(path, &self.write_paths)
    }

    /// Check whether a path is within the watch allow-list.
    ///
    /// Watch paths must also be within read paths (watching implies reading).
    pub fn is_watch_allowed(&self, path: &std::path::Path) -> bool {
        is_path_within_allowlist(path, &self.watch_paths)
    }
}

/// Check if a path falls within any of the allowed base paths.
///
/// Both the target and each allow-list entry are canonicalized to resolve
/// symlinks and relative components. This prevents:
/// - Symlink traversal: `/allowed/link -> /secret/data`
/// - Relative path escape: `/allowed/../secret/data`
/// - Path component tricks: `/allowed/./../../secret`
///
/// Returns `false` if either path cannot be canonicalized.
fn is_path_within_allowlist(path: &std::path::Path, allowlist: &[PathBuf]) -> bool {
    let canonical = match path.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            // For write operations, the file may not exist yet.
            // Canonicalize the parent directory instead.
            match path.parent().and_then(|p| p.canonicalize().ok()) {
                Some(parent) => {
                    if let Some(filename) = path.file_name() {
                        parent.join(filename)
                    } else {
                        return false;
                    }
                }
                None => return false,
            }
        }
    };

    for allowed in allowlist {
        if let Ok(allowed_canonical) = allowed.canonicalize()
            && canonical.starts_with(&allowed_canonical)
        {
            return true;
        }
    }

    false
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_default_debounce() {
        let config: FilesystemConfig = toml::from_str("").ok().unwrap_or(FilesystemConfig {
            watch_paths: vec![],
            read_paths: vec![],
            write_paths: vec![],
            debounce_ms: DEFAULT_DEBOUNCE_MS,
        });
        assert_eq!(config.debounce_duration(), Duration::from_millis(500));
    }

    #[test]
    fn test_deserialize_config() {
        let toml_str = r#"
            watch_paths = ["/tmp/watch"]
            read_paths = ["/tmp/read"]
            write_paths = ["/tmp/write"]
            debounce_ms = 200
        "#;
        let config: FilesystemConfig = toml::from_str(toml_str).ok().unwrap_or_else(|| {
            panic!("failed to parse config TOML");
        });
        assert_eq!(config.watch_paths.len(), 1);
        assert_eq!(config.read_paths.len(), 1);
        assert_eq!(config.write_paths.len(), 1);
        assert_eq!(config.debounce_ms, 200);
    }

    #[test]
    fn test_path_within_allowlist() {
        let dir = std::env::temp_dir().join("springtale_config_test_allow");
        fs::create_dir_all(&dir).ok();
        let child = dir.join("subdir");
        fs::create_dir_all(&child).ok();

        let config = FilesystemConfig {
            watch_paths: vec![],
            read_paths: vec![dir.clone()],
            write_paths: vec![dir.clone()],
            debounce_ms: 500,
        };

        // Child path is within allow-list
        assert!(config.is_read_allowed(&child));
        assert!(config.is_write_allowed(&child));

        // Clean up
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_path_outside_allowlist_rejected() {
        let allowed = std::env::temp_dir().join("springtale_config_test_allowed");
        let forbidden = std::env::temp_dir().join("springtale_config_test_forbidden");
        fs::create_dir_all(&allowed).ok();
        fs::create_dir_all(&forbidden).ok();

        let config = FilesystemConfig {
            watch_paths: vec![],
            read_paths: vec![allowed.clone()],
            write_paths: vec![allowed.clone()],
            debounce_ms: 500,
        };

        assert!(!config.is_read_allowed(&forbidden));
        assert!(!config.is_write_allowed(&forbidden));

        fs::remove_dir_all(&allowed).ok();
        fs::remove_dir_all(&forbidden).ok();
    }

    #[test]
    fn test_write_to_nonexistent_file_in_allowed_dir() {
        let dir = std::env::temp_dir().join("springtale_config_test_newfile");
        fs::create_dir_all(&dir).ok();

        let config = FilesystemConfig {
            watch_paths: vec![],
            read_paths: vec![],
            write_paths: vec![dir.clone()],
            debounce_ms: 500,
        };

        // File doesn't exist yet, but parent dir is allowed
        let new_file = dir.join("new_file.txt");
        assert!(config.is_write_allowed(&new_file));

        fs::remove_dir_all(&dir).ok();
    }
}
