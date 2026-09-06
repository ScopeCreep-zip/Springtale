use std::path::PathBuf;

use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::config::FilesystemConfig;
use crate::error::FilesystemError;

/// Action declaration for `write_file`.
pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: false,
        destructive: None,
        poll_interval_secs: None,
        name: "write_file".to_owned(),
        description: "Write content to a file. Path must be within the write allow-list."
            .to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute path to the file to write."
                },
                "content": {
                    "type": "string",
                    "description": "Content to write to the file."
                },
                "append": {
                    "type": "boolean",
                    "description": "If true, append to the file instead of overwriting. Default: false.",
                    "default": false
                }
            },
            "required": ["path", "content"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "bytes_written": {
                    "type": "integer",
                    "description": "Number of bytes written."
                }
            },
            "required": ["bytes_written"]
        })),
    }
}

/// Execute the `write_file` action.
///
/// Validates the path against the write allow-list, then writes or appends.
pub fn execute(
    config: &FilesystemConfig,
    input: &serde_json::Value,
) -> Result<ActionResult, FilesystemError> {
    let path_str = input
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| FilesystemError::InvalidInput("missing 'path' parameter".to_owned()))?;

    let content = input
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| FilesystemError::InvalidInput("missing 'content' parameter".to_owned()))?;

    let append = input
        .get("append")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let path = PathBuf::from(path_str);

    if !config.is_write_allowed(&path) {
        return Err(FilesystemError::PathNotAllowed(path_str.to_owned()));
    }

    let bytes_written = content.len();

    if append {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        file.write_all(content.as_bytes())?;
    } else {
        std::fs::write(&path, content)?;
    }

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({
            "bytes_written": bytes_written,
        }),
        message: format!(
            "{} {} bytes to {path_str}",
            if append { "appended" } else { "wrote" },
            bytes_written
        ),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_write_file_success() {
        let dir = std::env::temp_dir().join("springtale_action_write_test");
        fs::create_dir_all(&dir).ok();
        let file = dir.join("output.txt");

        let config = FilesystemConfig {
            watch_paths: vec![],
            read_paths: vec![],
            write_paths: vec![dir.clone()],
            debounce_ms: 500,
        };

        let input = serde_json::json!({
            "path": file.to_string_lossy(),
            "content": "hello world"
        });

        let result = execute(&config, &input).unwrap();
        assert!(result.success);
        assert_eq!(result.output["bytes_written"], 11);
        assert_eq!(fs::read_to_string(&file).unwrap(), "hello world");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_write_file_append() {
        let dir = std::env::temp_dir().join("springtale_action_write_append_test");
        fs::create_dir_all(&dir).ok();
        let file = dir.join("append.txt");
        fs::write(&file, "first ").ok();

        let config = FilesystemConfig {
            watch_paths: vec![],
            read_paths: vec![],
            write_paths: vec![dir.clone()],
            debounce_ms: 500,
        };

        let input = serde_json::json!({
            "path": file.to_string_lossy(),
            "content": "second",
            "append": true
        });

        let result = execute(&config, &input).unwrap();
        assert!(result.success);
        assert_eq!(fs::read_to_string(&file).unwrap(), "first second");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_write_file_path_not_allowed() {
        let allowed = std::env::temp_dir().join("springtale_action_write_allowed");
        let forbidden = std::env::temp_dir().join("springtale_action_write_forbidden");
        fs::create_dir_all(&allowed).ok();
        fs::create_dir_all(&forbidden).ok();

        let config = FilesystemConfig {
            watch_paths: vec![],
            read_paths: vec![],
            write_paths: vec![allowed.clone()],
            debounce_ms: 500,
        };

        let input = serde_json::json!({
            "path": forbidden.join("evil.txt").to_string_lossy(),
            "content": "evil"
        });

        let result = execute(&config, &input);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            FilesystemError::PathNotAllowed(_)
        ));

        fs::remove_dir_all(&allowed).ok();
        fs::remove_dir_all(&forbidden).ok();
    }

    #[test]
    fn test_write_file_missing_content() {
        let config = FilesystemConfig {
            watch_paths: vec![],
            read_paths: vec![],
            write_paths: vec![],
            debounce_ms: 500,
        };

        let input = serde_json::json!({ "path": "/tmp/test.txt" });
        let result = execute(&config, &input);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            FilesystemError::InvalidInput(_)
        ));
    }
}
