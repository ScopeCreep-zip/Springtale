use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::GithubApi;
use crate::error::GithubError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: false,
        destructive: None,
        poll_interval_secs: None,
        name: "create_branch".to_owned(),
        description: "Create a new branch from an existing base branch.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "owner": { "type": "string", "description": "Repository owner." },
                "repo": { "type": "string", "description": "Repository name." },
                "branch": { "type": "string", "description": "New branch name." },
                "base": { "type": "string", "description": "Base branch to fork from.", "default": "main" }
            },
            "required": ["owner", "repo", "branch"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "branch": { "type": "string" },
                "sha": { "type": "string" },
                "response": { "type": "object" }
            },
            "required": ["branch", "sha"]
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
    let base = input.get("base").and_then(|v| v.as_str()).unwrap_or("main");

    let base_sha = client.get_ref_sha(owner, repo, base).await?;
    let response = client.create_branch(owner, repo, branch, &base_sha).await?;

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({
            "branch": branch,
            "sha": base_sha,
            "response": response,
        }),
        message: format!("created branch '{branch}' from '{base}' in {owner}/{repo}"),
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
    async fn create_branch_resolves_base_then_creates() {
        let client = MockGithubClient::new(serde_json::json!({
            "object": { "sha": "abc123" },
            "ref": "refs/heads/feature"
        }));
        let input = serde_json::json!({
            "owner": "o", "repo": "r", "branch": "feature", "base": "main"
        });
        let result = execute(&client, &input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output["sha"], "abc123");
        assert_eq!(result.output["branch"], "feature");
    }

    #[tokio::test]
    async fn create_branch_missing_field_errors() {
        let client = MockGithubClient::new(serde_json::json!({}));
        let input = serde_json::json!({ "owner": "o", "repo": "r" });
        assert!(execute(&client, &input).await.is_err());
    }
}
