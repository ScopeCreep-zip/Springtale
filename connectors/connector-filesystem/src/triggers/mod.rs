use springtale_connector::manifest::types::TriggerDecl;

/// All trigger declarations for the filesystem connector.
pub fn trigger_declarations() -> Vec<TriggerDecl> {
    vec![file_created(), file_modified(), file_deleted()]
}

/// Trigger: a file was created in a watched directory.
fn file_created() -> TriggerDecl {
    TriggerDecl {
        name: "file_created".to_owned(),
        description: "Fires when a new file is created in a watched directory.".to_owned(),
        schema: Some(trigger_payload_schema()),
    }
}

/// Trigger: a file was modified in a watched directory.
fn file_modified() -> TriggerDecl {
    TriggerDecl {
        name: "file_modified".to_owned(),
        description: "Fires when a file is modified in a watched directory.".to_owned(),
        schema: Some(trigger_payload_schema()),
    }
}

/// Trigger: a file was deleted from a watched directory.
fn file_deleted() -> TriggerDecl {
    TriggerDecl {
        name: "file_deleted".to_owned(),
        description: "Fires when a file is deleted from a watched directory.".to_owned(),
        schema: Some(trigger_payload_schema()),
    }
}

/// JSON Schema for the event payload emitted by filesystem triggers.
///
/// All three triggers emit the same shape:
/// ```json
/// {
///   "path": "/absolute/path/to/file",
///   "event": "create" | "modify" | "delete",
///   "filename": "file.txt",
///   "extension": "txt"
/// }
/// ```
fn trigger_payload_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "path": {
                "type": "string",
                "description": "Absolute path to the affected file."
            },
            "event": {
                "type": "string",
                "enum": ["create", "modify", "delete"],
                "description": "The type of filesystem event."
            },
            "filename": {
                "type": "string",
                "description": "Name of the affected file (without directory)."
            },
            "extension": {
                "type": "string",
                "description": "File extension (empty string if none)."
            }
        },
        "required": ["path", "event", "filename", "extension"]
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_declarations_count() {
        let triggers = trigger_declarations();
        assert_eq!(triggers.len(), 3);
    }

    #[test]
    fn test_trigger_names() {
        let triggers = trigger_declarations();
        let names: Vec<&str> = triggers.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"file_created"));
        assert!(names.contains(&"file_modified"));
        assert!(names.contains(&"file_deleted"));
    }

    #[test]
    fn test_all_triggers_have_schemas() {
        let triggers = trigger_declarations();
        for trigger in &triggers {
            assert!(trigger.schema.is_some(), "trigger {} missing schema", trigger.name);
        }
    }
}
