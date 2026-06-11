use springtale_connector::manifest::types::TriggerDecl;

pub mod normalize;

/// All trigger declarations for the GitHub connector.
///
/// **Transformation contract:** These schemas describe the **transformed**
/// payload that springtaled's management API extracts from raw GitHub webhook
/// payloads — NOT the raw GitHub webhook JSON. For example, `repository` is
/// declared as a flat string (`"owner/repo"`) because the management API
/// extracts `repository.full_name` from the raw payload. Similarly, `author`
/// is extracted from `pull_request.user.login` or `issue.user.login`.
///
/// The raw GitHub webhook payloads have deeply nested structures. The
/// management API (M11) is responsible for:
/// 1. Verifying the webhook signature (via `webhook::verify_signature`)
/// 2. Extracting flat fields from the nested payload
/// 3. Dispatching the transformed payload to registered trigger handlers
pub fn trigger_declarations() -> Vec<TriggerDecl> {
    vec![
        push(),
        pull_request_opened(),
        issue_opened(),
        issue_comment(),
    ]
}

fn push() -> TriggerDecl {
    TriggerDecl {
        name: "push".to_owned(),
        description: "Fires when commits are pushed to a repository.".to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "ref": { "type": "string", "description": "Git ref (e.g., refs/heads/main)." },
                "repository": { "type": "string", "description": "Full repository name (owner/repo)." },
                "pusher": { "type": "string", "description": "Username of the pusher." },
                "commits_count": { "type": "integer", "description": "Number of commits pushed." }
            },
            "required": ["ref", "repository", "pusher"]
        })),
    }
}

fn pull_request_opened() -> TriggerDecl {
    TriggerDecl {
        name: "pull_request_opened".to_owned(),
        description: "Fires when a pull request is opened.".to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "number": { "type": "integer", "description": "PR number." },
                "title": { "type": "string", "description": "PR title." },
                "repository": { "type": "string", "description": "Full repository name." },
                "author": { "type": "string", "description": "PR author username." },
                "url": { "type": "string", "description": "PR HTML URL." }
            },
            "required": ["number", "title", "repository", "author"]
        })),
    }
}

fn issue_opened() -> TriggerDecl {
    TriggerDecl {
        name: "issue_opened".to_owned(),
        description: "Fires when an issue is opened.".to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "number": { "type": "integer", "description": "Issue number." },
                "title": { "type": "string", "description": "Issue title." },
                "repository": { "type": "string", "description": "Full repository name." },
                "author": { "type": "string", "description": "Issue author username." },
                "url": { "type": "string", "description": "Issue HTML URL." }
            },
            "required": ["number", "title", "repository", "author"]
        })),
    }
}

fn issue_comment() -> TriggerDecl {
    TriggerDecl {
        name: "issue_comment".to_owned(),
        description: "Fires when a comment is posted on an issue or PR.".to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "issue_number": { "type": "integer", "description": "Issue/PR number." },
                "body": { "type": "string", "description": "Comment body." },
                "repository": { "type": "string", "description": "Full repository name." },
                "author": { "type": "string", "description": "Comment author username." }
            },
            "required": ["issue_number", "body", "repository", "author"]
        })),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_count() {
        assert_eq!(trigger_declarations().len(), 4);
    }

    #[test]
    fn test_trigger_names() {
        let triggers = trigger_declarations();
        let names: Vec<&str> = triggers.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"push"));
        assert!(names.contains(&"pull_request_opened"));
        assert!(names.contains(&"issue_opened"));
        assert!(names.contains(&"issue_comment"));
    }

    #[test]
    fn test_all_triggers_have_schemas() {
        for trigger in trigger_declarations() {
            assert!(
                trigger.schema.is_some(),
                "trigger {} missing schema",
                trigger.name
            );
        }
    }
}
