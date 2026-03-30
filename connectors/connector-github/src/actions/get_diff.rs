use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::GithubApi;
use crate::error::GithubError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        name: "get_diff".to_owned(),
        description: "Get the unified diff for a GitHub pull request.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "owner": { "type": "string", "description": "Repository owner." },
                "repo": { "type": "string", "description": "Repository name." },
                "pull_number": { "type": "integer", "description": "Pull request number." }
            },
            "required": ["owner", "repo", "pull_number"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "diff": { "type": "string", "description": "Unified diff content." }
            },
            "required": ["diff"]
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
    let pull_number = input
        .get("pull_number")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| GithubError::InvalidInput("missing 'pull_number'".to_owned()))?;

    let diff = client.get_diff(owner, repo, pull_number).await?;

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({
            "diff": diff,
        }),
        message: format!("fetched diff for {owner}/{repo}#{pull_number}"),
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
        assert_eq!(decl.name, "get_diff");
        let schema = decl.input_schema.unwrap();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("pull_number")));
    }

    #[tokio::test]
    async fn test_execute_missing_owner_returns_invalid_input() {
        let client = test_client();
        let input = serde_json::json!({ "repo": "r", "pull_number": 1 });
        let result = execute(&client, &input).await;
        assert!(matches!(result.unwrap_err(), GithubError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_execute_missing_repo_returns_invalid_input() {
        let client = test_client();
        let input = serde_json::json!({ "owner": "o", "pull_number": 1 });
        let result = execute(&client, &input).await;
        assert!(matches!(result.unwrap_err(), GithubError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_execute_missing_pull_number_returns_invalid_input() {
        let client = test_client();
        let input = serde_json::json!({ "owner": "o", "repo": "r" });
        let result = execute(&client, &input).await;
        assert!(matches!(result.unwrap_err(), GithubError::InvalidInput(_)));
    }
}
