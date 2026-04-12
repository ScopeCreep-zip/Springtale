use crate::config::ShellConfig;
use crate::error::ShellError;

/// Validate a command against the shell connector's sandbox rules.
///
/// The sandbox enforces:
/// 1. Only allow-listed commands can be executed
/// 2. No shell metacharacters that could escape the allow-list
///    (pipes, redirects, backticks, subshells, semicolons)
///
/// This is NOT a full shell sandbox — it prevents the most common
/// injection vectors. The real security boundary is the `ShellExec`
/// capability which requires explicit user approval.
pub fn validate_command(
    config: &ShellConfig,
    command: &str,
    args: &[String],
) -> Result<(), ShellError> {
    // Check command is in the allow-list
    if !config.is_command_allowed(command) {
        return Err(ShellError::CommandNotAllowed(command.to_owned()));
    }

    // Check for shell metacharacters in arguments that could be used
    // for injection. We execute commands directly (not via shell), so
    // these characters shouldn't normally appear in legitimate arguments.
    let dangerous_patterns = [
        "|", ";", "`", "$(", "${", ">>", "<<", "&&", "||", ">", "<", "&", "\n", "\r", "\0",
    ];

    for arg in args {
        for pattern in &dangerous_patterns {
            if arg.contains(pattern) {
                return Err(ShellError::CommandNotAllowed(format!(
                    "argument contains shell metacharacter '{pattern}': {arg}"
                )));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn test_config() -> ShellConfig {
        ShellConfig {
            allowed_commands: vec!["echo".to_owned(), "ls".to_owned(), "cat".to_owned()],
            timeout_secs: 30,
            working_directory: None,
        }
    }

    #[test]
    fn test_allowed_command_passes() {
        let config = test_config();
        assert!(validate_command(&config, "echo", &["hello".to_owned()]).is_ok());
    }

    #[test]
    fn test_disallowed_command_rejected() {
        let config = test_config();
        let result = validate_command(&config, "rm", &["-rf".to_owned(), "/".to_owned()]);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ShellError::CommandNotAllowed(_)
        ));
    }

    #[test]
    fn test_pipe_injection_rejected() {
        let config = test_config();
        let result = validate_command(&config, "echo", &["hello | rm -rf /".to_owned()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_semicolon_injection_rejected() {
        let config = test_config();
        let result = validate_command(&config, "echo", &["hello; rm -rf /".to_owned()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_backtick_injection_rejected() {
        let config = test_config();
        let result = validate_command(&config, "echo", &["`rm -rf /`".to_owned()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_subshell_injection_rejected() {
        let config = test_config();
        let result = validate_command(&config, "echo", &["$(rm -rf /)".to_owned()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_args_passes() {
        let config = test_config();
        assert!(validate_command(&config, "ls", &[]).is_ok());
    }

    #[test]
    fn test_safe_args_pass() {
        let config = test_config();
        assert!(validate_command(&config, "ls", &["-la".to_owned(), "/tmp".to_owned()]).is_ok());
    }

    #[test]
    fn test_redirect_rejected() {
        let config = test_config();
        assert!(validate_command(&config, "echo", &["test > /tmp/file".to_owned()]).is_err());
    }

    #[test]
    fn test_input_redirect_rejected() {
        let config = test_config();
        assert!(validate_command(&config, "cat", &["< /etc/passwd".to_owned()]).is_err());
    }

    #[test]
    fn test_newline_injection_rejected() {
        let config = test_config();
        assert!(validate_command(&config, "echo", &["test\nrm -rf /".to_owned()]).is_err());
    }

    #[test]
    fn test_background_rejected() {
        let config = test_config();
        assert!(validate_command(&config, "echo", &["test & rm /".to_owned()]).is_err());
    }

    #[test]
    fn test_null_byte_rejected() {
        let config = test_config();
        assert!(validate_command(&config, "echo", &["test\0evil".to_owned()]).is_err());
    }
}
