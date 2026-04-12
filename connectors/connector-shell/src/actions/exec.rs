use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;
use tokio::process::Command;

use crate::config::ShellConfig;
use crate::error::ShellError;
use crate::sandbox;

/// Action declaration for `exec`.
pub fn declaration() -> ActionDecl {
    ActionDecl {
        name: "exec".to_owned(),
        description: "Execute an allow-listed shell command with a timeout. Requires ShellExec capability approval.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The command to execute (must be in the allow-list)."
                },
                "args": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Arguments to pass to the command.",
                    "default": []
                }
            },
            "required": ["command"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "exit_code": {
                    "type": "integer",
                    "description": "Process exit code (0 = success)."
                },
                "stdout": {
                    "type": "string",
                    "description": "Standard output of the command."
                },
                "stderr": {
                    "type": "string",
                    "description": "Standard error of the command."
                }
            },
            "required": ["exit_code", "stdout", "stderr"]
        })),
    }
}

/// Execute the `exec` action.
///
/// Validates the command against the allow-list and sandbox rules,
/// then runs it with `tokio::process::Command` with a timeout.
pub async fn execute(
    config: &ShellConfig,
    input: &serde_json::Value,
) -> Result<ActionResult, ShellError> {
    let command = input
        .get("command")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ShellError::InvalidInput("missing 'command' parameter".to_owned()))?;

    let args: Vec<String> = input
        .get("args")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    // Validate against sandbox rules (allow-list + metacharacter check)
    sandbox::validate_command(config, command, &args)?;

    tracing::info!(
        command = command,
        args = ?args,
        timeout_secs = config.timeout_secs,
        "executing shell command"
    );

    // Build the command — runs directly, NOT through a shell
    let mut cmd = Command::new(command);
    cmd.args(&args);

    // Set working directory if configured — validate against path traversal
    if let Some(ref wd) = config.working_directory {
        let path = std::path::Path::new(wd);
        if !path.is_absolute() {
            return Err(crate::error::ShellError::InvalidConfig(
                "working_directory must be absolute".into(),
            ));
        }
        if path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(crate::error::ShellError::InvalidConfig(
                "working_directory must not contain '..'".into(),
            ));
        }
        cmd.current_dir(path);
    }

    // Capture stdout and stderr
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    // Spawn and wait with timeout
    let child = cmd.spawn()?;
    let timeout = config.timeout_duration();

    let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(result) => result?,
        Err(_) => {
            return Err(ShellError::Timeout(config.timeout_secs));
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let exit_code = output.status.code().unwrap_or(-1);

    let success = output.status.success();

    Ok(ActionResult {
        success,
        output: serde_json::json!({
            "exit_code": exit_code,
            "stdout": stdout,
            "stderr": stderr,
        }),
        message: format!("command '{command}' exited with code {exit_code}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn test_config() -> ShellConfig {
        ShellConfig {
            allowed_commands: vec!["echo".to_owned(), "true".to_owned(), "false".to_owned()],
            timeout_secs: 5,
            working_directory: None,
        }
    }

    #[tokio::test]
    async fn test_exec_echo() {
        let config = test_config();
        let input = serde_json::json!({
            "command": "echo",
            "args": ["hello", "world"]
        });

        let result = execute(&config, &input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output["exit_code"], 0);
        assert_eq!(
            result.output["stdout"].as_str().unwrap().trim(),
            "hello world"
        );
    }

    #[tokio::test]
    async fn test_exec_command_not_allowed() {
        let config = test_config();
        let input = serde_json::json!({
            "command": "rm",
            "args": ["-rf", "/"]
        });

        let result = execute(&config, &input).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ShellError::CommandNotAllowed(_)
        ));
    }

    #[tokio::test]
    async fn test_exec_false_returns_failure() {
        let config = test_config();
        let input = serde_json::json!({
            "command": "false"
        });

        let result = execute(&config, &input).await.unwrap();
        assert!(!result.success);
        assert_ne!(result.output["exit_code"], 0);
    }

    #[tokio::test]
    async fn test_exec_missing_command_param() {
        let config = test_config();
        let input = serde_json::json!({});

        let result = execute(&config, &input).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ShellError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_exec_timeout() {
        let config = ShellConfig {
            allowed_commands: vec!["sleep".to_owned()],
            timeout_secs: 1,
            working_directory: None,
        };

        let input = serde_json::json!({
            "command": "sleep",
            "args": ["30"]
        });

        let result = execute(&config, &input).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ShellError::Timeout(_)));
    }

    #[tokio::test]
    async fn test_exec_no_args() {
        let config = test_config();
        let input = serde_json::json!({
            "command": "true"
        });

        let result = execute(&config, &input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output["exit_code"], 0);
    }
}
