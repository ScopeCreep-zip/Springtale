use serde::Deserialize;

/// Default command timeout in seconds.
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Configuration for the shell connector.
///
/// Deserialized from TOML config. Never serialized.
#[derive(Debug, Clone, Deserialize)]
pub struct ShellConfig {
    /// Commands that are allowed to be executed.
    /// Only the command name (first argument) is checked against this list.
    /// If empty, no commands can be executed.
    #[serde(default)]
    pub allowed_commands: Vec<String>,

    /// Maximum execution time per command in seconds. Default: 30s.
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,

    /// Working directory for command execution. If not set, inherits the
    /// process working directory.
    #[serde(default)]
    pub working_directory: Option<String>,
}

fn default_timeout_secs() -> u64 {
    DEFAULT_TIMEOUT_SECS
}

impl ShellConfig {
    /// Get the timeout as a `Duration`.
    pub fn timeout_duration(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.timeout_secs)
    }

    /// Check whether a command name is in the allow-list.
    pub fn is_command_allowed(&self, command: &str) -> bool {
        self.allowed_commands.iter().any(|c| c == command)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_command_allowed() {
        let config = ShellConfig {
            allowed_commands: vec!["ls".to_owned(), "echo".to_owned()],
            timeout_secs: 30,
            working_directory: None,
        };

        assert!(config.is_command_allowed("ls"));
        assert!(config.is_command_allowed("echo"));
        assert!(!config.is_command_allowed("rm"));
        assert!(!config.is_command_allowed(""));
    }

    #[test]
    fn test_empty_allowlist_blocks_all() {
        let config = ShellConfig {
            allowed_commands: vec![],
            timeout_secs: 30,
            working_directory: None,
        };

        assert!(!config.is_command_allowed("ls"));
    }

    #[test]
    fn test_default_timeout() {
        let config = ShellConfig {
            allowed_commands: vec![],
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            working_directory: None,
        };

        assert_eq!(
            config.timeout_duration(),
            std::time::Duration::from_secs(30)
        );
    }
}
