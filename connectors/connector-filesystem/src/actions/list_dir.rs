use std::path::PathBuf;

use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::config::FilesystemConfig;
use crate::error::FilesystemError;

/// Action declaration for `list_dir`.
pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: true,
        name: "list_dir".to_owned(),
        description: "List the contents of a directory. Path must be within the read allow-list."
            .to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute path to the directory to list."
                }
            },
            "required": ["path"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "entries": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" },
                            "path": { "type": "string" },
                            "is_dir": { "type": "boolean" },
                            "is_file": { "type": "boolean" },
                            "size_bytes": { "type": "integer" }
                        }
                    },
                    "description": "List of directory entries."
                },
                "count": {
                    "type": "integer",
                    "description": "Number of entries."
                }
            },
            "required": ["entries", "count"]
        })),
    }
}

/// Execute the `list_dir` action.
///
/// Validates the path against the read allow-list, then lists directory entries.
/// Does not follow symlinks — uses `symlink_metadata` to avoid traversal.
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

    if !path.is_dir() {
        return Err(FilesystemError::InvalidInput(format!(
            "path is not a directory: {path_str}"
        )));
    }

    let mut entries = Vec::new();

    for entry in std::fs::read_dir(&path)? {
        let entry = entry?;
        // Use symlink_metadata to NOT follow symlinks — prevents traversal
        let metadata = std::fs::symlink_metadata(entry.path())?;
        let entry_path = entry.path();

        // Skip symlinks entirely to prevent traversal
        if metadata.file_type().is_symlink() {
            tracing::debug!(
                path = %entry_path.display(),
                "skipping symlink in directory listing"
            );
            continue;
        }

        entries.push(serde_json::json!({
            "name": entry.file_name().to_string_lossy(),
            "path": entry_path.to_string_lossy(),
            "is_dir": metadata.is_dir(),
            "is_file": metadata.is_file(),
            "size_bytes": metadata.len(),
        }));
    }

    let count = entries.len();

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({
            "entries": entries,
            "count": count,
        }),
        message: format!("listed {count} entries in {path_str}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_list_dir_success() {
        let dir = std::env::temp_dir().join("springtale_action_listdir_test");
        fs::create_dir_all(&dir).ok();
        fs::write(dir.join("a.txt"), "aaa").ok();
        fs::write(dir.join("b.txt"), "bbb").ok();
        fs::create_dir_all(dir.join("subdir")).ok();

        let config = FilesystemConfig {
            watch_paths: vec![],
            read_paths: vec![dir.clone()],
            write_paths: vec![],
            debounce_ms: 500,
        };

        let input = serde_json::json!({ "path": dir.to_string_lossy() });
        let result = execute(&config, &input).unwrap();

        assert!(result.success);
        assert_eq!(result.output["count"], 3);

        let entries = result.output["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 3);

        // Verify we have at least one file and one directory
        let has_file = entries.iter().any(|e| e["is_file"] == true);
        let has_dir = entries.iter().any(|e| e["is_dir"] == true);
        assert!(has_file);
        assert!(has_dir);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_list_dir_not_allowed() {
        let allowed = std::env::temp_dir().join("springtale_action_listdir_allowed");
        let forbidden = std::env::temp_dir().join("springtale_action_listdir_forbidden");
        fs::create_dir_all(&allowed).ok();
        fs::create_dir_all(&forbidden).ok();

        let config = FilesystemConfig {
            watch_paths: vec![],
            read_paths: vec![allowed.clone()],
            write_paths: vec![],
            debounce_ms: 500,
        };

        let input = serde_json::json!({ "path": forbidden.to_string_lossy() });
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
    fn test_list_dir_not_found() {
        let dir = std::env::temp_dir().join("springtale_action_listdir_notfound");
        // Don't create it — ensure it doesn't exist
        fs::remove_dir_all(&dir).ok();

        let config = FilesystemConfig {
            watch_paths: vec![],
            read_paths: vec![std::env::temp_dir()],
            write_paths: vec![],
            debounce_ms: 500,
        };

        let input = serde_json::json!({ "path": dir.to_string_lossy() });
        let result = execute(&config, &input);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            FilesystemError::PathNotFound(_)
        ));
    }

    #[test]
    fn test_list_dir_not_a_directory() {
        let dir = std::env::temp_dir().join("springtale_action_listdir_file_test");
        fs::create_dir_all(&dir).ok();
        let file = dir.join("not_a_dir.txt");
        fs::write(&file, "data").ok();

        let config = FilesystemConfig {
            watch_paths: vec![],
            read_paths: vec![dir.clone()],
            write_paths: vec![],
            debounce_ms: 500,
        };

        let input = serde_json::json!({ "path": file.to_string_lossy() });
        let result = execute(&config, &input);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            FilesystemError::InvalidInput(_)
        ));

        fs::remove_dir_all(&dir).ok();
    }
}
