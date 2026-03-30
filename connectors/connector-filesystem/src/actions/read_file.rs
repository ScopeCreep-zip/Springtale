use std::path::PathBuf;

use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::config::FilesystemConfig;
use crate::error::FilesystemError;

/// Action declaration for `read_file`.
pub fn declaration() -> ActionDecl {
    ActionDecl {
        name: "read_file".to_owned(),
        description: "Read the contents of a file. Path must be within the read allow-list."
            .to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute path to the file to read."
                }
            },
            "required": ["path"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The file contents as a UTF-8 string."
                },
                "size_bytes": {
                    "type": "integer",
                    "description": "File size in bytes."
                }
            },
            "required": ["content", "size_bytes"]
        })),
    }
}

/// Execute the `read_file` action.
///
/// Validates the path against the read allow-list, then reads the file.
/// Returns the file content as a UTF-8 string and the file size.
pub fn execute(
    config: &FilesystemConfig,
    input: &serde_json::Value,
) -> Result<ActionResult, FilesystemError> {
    let path_str = input
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| FilesystemError::InvalidInput("missing 'path' parameter".to_owned()))?;

    let path = PathBuf::from(path_str);

    if !config.is_read_allowed(&path) {
        return Err(FilesystemError::PathNotAllowed(path_str.to_owned()));
    }

    if !path.exists() {
        return Err(FilesystemError::PathNotFound(path_str.to_owned()));
    }

    let content = std::fs::read_to_string(&path)?;
    let size_bytes = content.len();

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({
            "content": content,
            "size_bytes": size_bytes,
        }),
        message: format!("read {} bytes from {path_str}", size_bytes),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_read_file_success() {
        let dir = std::env::temp_dir().join("springtale_action_read_test");
        fs::create_dir_all(&dir).ok();
        let file = dir.join("test.txt");
        fs::write(&file, "hello world").ok();

        let config = FilesystemConfig {
            watch_paths: vec![],
            read_paths: vec![dir.clone()],
            write_paths: vec![],
            debounce_ms: 500,
        };

        let input = serde_json::json!({ "path": file.to_string_lossy() });
        let result = execute(&config, &input).unwrap();

        assert!(result.success);
        assert_eq!(result.output["content"], "hello world");
        assert_eq!(result.output["size_bytes"], 11);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_read_file_path_not_allowed() {
        let allowed = std::env::temp_dir().join("springtale_action_read_allowed");
        let forbidden = std::env::temp_dir().join("springtale_action_read_forbidden");
        fs::create_dir_all(&allowed).ok();
        fs::create_dir_all(&forbidden).ok();
        let file = forbidden.join("secret.txt");
        fs::write(&file, "secret data").ok();

        let config = FilesystemConfig {
            watch_paths: vec![],
            read_paths: vec![allowed.clone()],
            write_paths: vec![],
            debounce_ms: 500,
        };

        let input = serde_json::json!({ "path": file.to_string_lossy() });
        let result = execute(&config, &input);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), FilesystemError::PathNotAllowed(_)));

        fs::remove_dir_all(&allowed).ok();
        fs::remove_dir_all(&forbidden).ok();
    }

    #[test]
    fn test_read_file_not_found() {
        let dir = std::env::temp_dir().join("springtale_action_read_notfound");
        fs::create_dir_all(&dir).ok();

        let config = FilesystemConfig {
            watch_paths: vec![],
            read_paths: vec![dir.clone()],
            write_paths: vec![],
            debounce_ms: 500,
        };

        let input = serde_json::json!({ "path": dir.join("nonexistent.txt").to_string_lossy() });
        let result = execute(&config, &input);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), FilesystemError::PathNotFound(_)));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_read_file_missing_path_param() {
        let config = FilesystemConfig {
            watch_paths: vec![],
            read_paths: vec![],
            write_paths: vec![],
            debounce_ms: 500,
        };

        let input = serde_json::json!({});
        let result = execute(&config, &input);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), FilesystemError::InvalidInput(_)));
    }
}
