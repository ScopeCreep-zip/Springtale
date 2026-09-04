use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::GithubApi;
use crate::error::GithubError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: false,
        destructive: None,
        name: "post_comment".to_owned(),
        description: "Post a comment on a GitHub issue or pull request.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "owner": { "type": "string", "description": "Repository owner." },
                "repo": { "type": "string", "description": "Repository name." },
                "issue_number": { "type": "integer", "description": "Issue or PR number." },
                "body": { "type": "string", "description": "Comment body (Markdown)." }
            },
            "required": ["owner", "repo", "issue_number", "body"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "id": { "type": "integer" },
                "url": { "type": "string" },
                "response": { "type": "object" }
            },
            "required": ["id", "url"]
        })),
    }
}

pub async fn execute(
    client: &dyn GithubApi,
    input: &serde_json::Value,
) -> Result<ActionResult, GithubError> {
    let owner = input
        .get("owner")
        .and_then(|v| v.as_str())
        .ok_or_else(|| GithubError::InvalidInput("missing 'owner'".to_owned()))?;
    let repo = input
        .get("repo")
        .and_then(|v| v.as_str())
        .ok_or_else(|| GithubError::InvalidInput("missing 'repo'".to_owned()))?;
    let issue_number = input
        .get("issue_number")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| GithubError::InvalidInput("missing 'issue_number'".to_owned()))?;
    let body = input
        .get("body")
        .and_then(|v| v.as_str())
        .ok_or_else(|| GithubError::InvalidInput("missing 'body'".to_owned()))?;

    let response = client.post_comment(owner, repo, issue_number, body).await?;

    let id = response.get("id").and_then(|n| n.as_u64()).unwrap_or(0);
    let url = response
        .get("html_url")
        .and_then(|u| u.as_str())
        .unwrap_or("")
        .to_owned();

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({
            "id": id,
            "url": url,
            "response": response,
        }),
        message: format!("posted comment on {owner}/{repo}#{issue_number}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::GithubClient;

    fn test_client() -> GithubClient {
        let config = crate::config::GithubConfig {
            token: secrecy::SecretBox::new(Box::new("fake".to_owned())),
            webhook_secret: None,
            api_base: "http://localhost:0".to_owned(),
        };
        GithubClient::new(&config).unwrap()
    }

    #[test]
    fn test_declaration() {
        let decl = declaration();
        assert_eq!(decl.name, "post_comment");
        let schema = decl.input_schema.unwrap();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("body")));
        assert!(required.contains(&serde_json::json!("issue_number")));
    }

    #[tokio::test]
    async fn test_execute_missing_owner_returns_invalid_input() {
        let client = test_client();
        let input = serde_json::json!({ "repo": "r", "issue_number": 1, "body": "b" });
        let result = execute(&client, &input).await;
        assert!(matches!(result.unwrap_err(), GithubError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_execute_missing_repo_returns_invalid_input() {
        let client = test_client();
        let input = serde_json::json!({ "owner": "o", "issue_number": 1, "body": "b" });
        let result = execute(&client, &input).await;
        assert!(matches!(result.unwrap_err(), GithubError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_execute_missing_issue_number_returns_invalid_input() {
        let client = test_client();
        let input = serde_json::json!({ "owner": "o", "repo": "r", "body": "b" });
        let result = execute(&client, &input).await;
        assert!(matches!(result.unwrap_err(), GithubError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_execute_missing_body_returns_invalid_input() {
        let client = test_client();
        let input = serde_json::json!({ "owner": "o", "repo": "r", "issue_number": 1 });
        let result = execute(&client, &input).await;
        assert!(matches!(result.unwrap_err(), GithubError::InvalidInput(_)));
    }
}
