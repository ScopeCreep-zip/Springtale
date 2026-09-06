use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::GithubApi;
use crate::error::GithubError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: false,
        destructive: None,
        poll_interval_secs: None,
        name: "commit_file".to_owned(),
        description: "Create or update a single file on a branch with a commit.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "owner": { "type": "string", "description": "Repository owner." },
                "repo": { "type": "string", "description": "Repository name." },
                "branch": { "type": "string", "description": "Branch to commit on." },
                "path": { "type": "string", "description": "File path within the repo." },
                "content": { "type": "string", "description": "New file content (plain UTF-8)." },
                "message": { "type": "string", "description": "Commit message." },
                "existing_sha": { "type": "string", "description": "Blob SHA when updating an existing file (omit to create)." }
            },
            "required": ["owner", "repo", "branch", "path", "content", "message"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "commit_sha": { "type": "string" },
                "response": { "type": "object" }
            },
            "required": ["path"]
        })),
    }
}

pub async fn execute(
    client: &dyn GithubApi,
    input: &serde_json::Value,
) -> Result<ActionResult, GithubError> {
    let owner = require_str(input, "owner")?;
    let repo = require_str(input, "repo")?;
    let branch = require_str(input, "branch")?;
    let path = require_str(input, "path")?;
    let content = require_str(input, "content")?;
    let message = require_str(input, "message")?;
    let existing_sha = input.get("existing_sha").and_then(|v| v.as_str());

    let response = client
        .commit_file(
            owner,
            repo,
            branch,
            path,
            content.as_bytes(),
            message,
            existing_sha,
        )
        .await?;

    let commit_sha = response
        .pointer("/commit/sha")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_owned();

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({
            "path": path,
            "commit_sha": commit_sha,
            "response": response,
        }),
        message: format!("committed '{path}' on '{branch}' in {owner}/{repo}"),
    })
}

fn require_str<'a>(input: &'a serde_json::Value, key: &str) -> Result<&'a str, GithubError> {
    input
        .get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| GithubError::InvalidInput(format!("missing '{key}'")))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::actions::test_support::MockGithubClient;

    #[tokio::test]
    async fn commit_file_encodes_and_returns_sha() {
        let client = MockGithubClient::new(serde_json::json!({
            "commit": { "sha": "deadbeef" }
        }));
        let input = serde_json::json!({
            "owner": "o", "repo": "r", "branch": "feat",
            "path": "README.md", "content": "hello world", "message": "docs"
        });
        let result = execute(&client, &input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output["commit_sha"], "deadbeef");
        assert_eq!(result.output["path"], "README.md");
    }

    #[tokio::test]
    async fn commit_file_missing_content_errors() {
        let client = MockGithubClient::new(serde_json::json!({}));
        let input = serde_json::json!({
            "owner": "o", "repo": "r", "branch": "feat", "path": "x", "message": "m"
        });
        assert!(execute(&client, &input).await.is_err());
    }
}
