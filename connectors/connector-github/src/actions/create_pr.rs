use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::GithubApi;
use crate::error::GithubError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: false,
        name: "create_pr".to_owned(),
        description: "Open a pull request from a head branch into a base branch.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "owner": { "type": "string", "description": "Repository owner." },
                "repo": { "type": "string", "description": "Repository name." },
                "title": { "type": "string", "description": "Pull request title." },
                "head": { "type": "string", "description": "Branch with the changes." },
                "base": { "type": "string", "description": "Branch to merge into.", "default": "main" },
                "body": { "type": "string", "description": "Pull request body (Markdown).", "default": "" }
            },
            "required": ["owner", "repo", "title", "head"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "number": { "type": "integer" },
                "url": { "type": "string" },
                "response": { "type": "object" }
            },
            "required": ["number", "url"]
        })),
    }
}

pub async fn execute(
    client: &dyn GithubApi,
    input: &serde_json::Value,
) -> Result<ActionResult, GithubError> {
    let owner = require_str(input, "owner")?;
    let repo = require_str(input, "repo")?;
    let title = require_str(input, "title")?;
    let head = require_str(input, "head")?;
    let base = input.get("base").and_then(|v| v.as_str()).unwrap_or("main");
    let body = input.get("body").and_then(|v| v.as_str()).unwrap_or("");

    let response = client
        .create_pr(owner, repo, title, head, base, body)
        .await?;

    let number = response.get("number").and_then(|n| n.as_u64()).unwrap_or(0);
    let url = response
        .get("html_url")
        .and_then(|u| u.as_str())
        .unwrap_or("")
        .to_owned();

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({
            "number": number,
            "url": url,
            "response": response,
        }),
        message: format!("opened PR #{number}: {head} → {base} in {owner}/{repo}"),
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
    async fn create_pr_returns_number_and_url() {
        let client = MockGithubClient::new(serde_json::json!({
            "number": 7, "html_url": "https://github.com/o/r/pull/7"
        }));
        let input = serde_json::json!({
            "owner": "o", "repo": "r", "title": "Add feature", "head": "feat", "base": "main"
        });
        let result = execute(&client, &input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output["number"], 7);
        assert_eq!(result.output["url"], "https://github.com/o/r/pull/7");
    }

    #[tokio::test]
    async fn create_pr_missing_head_errors() {
        let client = MockGithubClient::new(serde_json::json!({}));
        let input = serde_json::json!({ "owner": "o", "repo": "r", "title": "t" });
        assert!(execute(&client, &input).await.is_err());
    }
}
