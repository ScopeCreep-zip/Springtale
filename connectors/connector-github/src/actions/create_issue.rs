use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::GithubApi;
use crate::error::GithubError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        name: "create_issue".to_owned(),
        description: "Create a new issue in a GitHub repository.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "owner": { "type": "string", "description": "Repository owner." },
                "repo": { "type": "string", "description": "Repository name." },
                "title": { "type": "string", "description": "Issue title." },
                "body": { "type": "string", "description": "Issue body (Markdown).", "default": "" }
            },
            "required": ["owner", "repo", "title"]
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
    let owner = input.get("owner").and_then(|v| v.as_str())
        .ok_or_else(|| GithubError::InvalidInput("missing 'owner'".to_owned()))?;
    let repo = input.get("repo").and_then(|v| v.as_str())
        .ok_or_else(|| GithubError::InvalidInput("missing 'repo'".to_owned()))?;
    let title = input.get("title").and_then(|v| v.as_str())
        .ok_or_else(|| GithubError::InvalidInput("missing 'title'".to_owned()))?;
    let body = input.get("body").and_then(|v| v.as_str()).unwrap_or("");

    let response = client.create_issue(owner, repo, title, body).await?;

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
        message: format!("created issue #{number} in {owner}/{repo}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::GithubClient;

    /// Mock client that returns canned responses for testing action logic.
    struct MockGithubClient {
        response: serde_json::Value,
    }

    #[async_trait::async_trait]
    impl GithubApi for MockGithubClient {
        async fn create_issue(
            &self,
            _owner: &str,
            _repo: &str,
            _title: &str,
            _body: &str,
        ) -> Result<serde_json::Value, GithubError> {
            Ok(self.response.clone())
        }

        async fn post_comment(
            &self,
            _owner: &str,
            _repo: &str,
            _issue_number: u64,
            _body: &str,
        ) -> Result<serde_json::Value, GithubError> {
            Ok(self.response.clone())
        }

        async fn get_diff(
            &self,
            _owner: &str,
            _repo: &str,
            _pull_number: u64,
        ) -> Result<String, GithubError> {
            Ok(self.response.to_string())
        }
    }

    fn real_test_client() -> GithubClient {
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
        assert_eq!(decl.name, "create_issue");
        let schema = decl.input_schema.unwrap();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("owner")));
        assert!(required.contains(&serde_json::json!("repo")));
        assert!(required.contains(&serde_json::json!("title")));
    }

    // --- Input validation tests (use real client, never reaches network) ---

    #[tokio::test]
    async fn test_execute_missing_owner_returns_invalid_input() {
        let client = real_test_client();
        let input = serde_json::json!({ "repo": "r", "title": "t" });
        let result = execute(&client, &input).await;
        assert!(matches!(result.unwrap_err(), GithubError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_execute_missing_repo_returns_invalid_input() {
        let client = real_test_client();
        let input = serde_json::json!({ "owner": "o", "title": "t" });
        let result = execute(&client, &input).await;
        assert!(matches!(result.unwrap_err(), GithubError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_execute_missing_title_returns_invalid_input() {
        let client = real_test_client();
        let input = serde_json::json!({ "owner": "o", "repo": "r" });
        let result = execute(&client, &input).await;
        assert!(matches!(result.unwrap_err(), GithubError::InvalidInput(_)));
    }

    // --- Mock client tests: verify response extraction logic ---

    #[tokio::test]
    async fn test_execute_extracts_number_and_url_from_response() {
        let mock = MockGithubClient {
            response: serde_json::json!({
                "number": 42,
                "html_url": "https://github.com/owner/repo/issues/42",
                "id": 123456,
                "title": "Test Issue"
            }),
        };

        let input = serde_json::json!({
            "owner": "owner",
            "repo": "repo",
            "title": "Test Issue",
            "body": "Test body"
        });

        let result = execute(&mock, &input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output["number"], 42);
        assert_eq!(
            result.output["url"],
            "https://github.com/owner/repo/issues/42"
        );
        assert!(result.message.contains("#42"));
        assert!(result.message.contains("owner/repo"));
    }

    #[tokio::test]
    async fn test_execute_handles_missing_fields_in_response() {
        let mock = MockGithubClient {
            response: serde_json::json!({
                "id": 999
                // no "number" or "html_url" — should default to 0 and ""
            }),
        };

        let input = serde_json::json!({
            "owner": "o",
            "repo": "r",
            "title": "t"
        });

        let result = execute(&mock, &input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output["number"], 0);
        assert_eq!(result.output["url"], "");
    }
}
