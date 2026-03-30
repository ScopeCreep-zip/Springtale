use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretBox};
use springtale_connector::client::handle_json_response;

use crate::config::GithubConfig;
use crate::error::GithubError;

/// Trait defining the GitHub API surface.
///
/// Actions depend on this trait, not the concrete client. This enables
/// mock implementations in tests (per testing.md: "mock at the client
/// layer, not at reqwest level").
#[async_trait]
pub trait GithubApi: Send + Sync {
    async fn create_issue(
        &self,
        owner: &str,
        repo: &str,
        title: &str,
        body: &str,
    ) -> Result<serde_json::Value, GithubError>;

    async fn post_comment(
        &self,
        owner: &str,
        repo: &str,
        issue_number: u64,
        body: &str,
    ) -> Result<serde_json::Value, GithubError>;

    async fn get_diff(
        &self,
        owner: &str,
        repo: &str,
        pull_number: u64,
    ) -> Result<String, GithubError>;
}

/// GitHub REST API v3 client.
///
/// All GitHub API calls go through this client. It sets the required
/// authentication and accept headers per GitHub's API docs.
pub struct GithubClient {
    inner: reqwest::Client,
    api_base: String,
    auth_token: SecretBox<String>,
}

impl GithubClient {
    /// Create a new GitHub API client from config.
    pub fn new(config: &GithubConfig) -> Result<Self, GithubError> {
        let inner = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| GithubError::RequestFailed(format!("failed to build client: {e}")))?;

        Ok(Self {
            inner,
            api_base: config.api_base.clone(),
            auth_token: config.token_clone(),
        })
    }

    /// Build the Authorization header value at point of use.
    fn auth_header(&self) -> String {
        // SECURITY: expose needed for Authorization header on each API call
        format!("Bearer {}", self.auth_token.expose_secret())
    }

}

#[async_trait]
impl GithubApi for GithubClient {
    async fn create_issue(
        &self,
        owner: &str,
        repo: &str,
        title: &str,
        body: &str,
    ) -> Result<serde_json::Value, GithubError> {
        let url = format!("{}/repos/{owner}/{repo}/issues", self.api_base);

        let response = self
            .inner
            .post(&url)
            .header("Authorization", self.auth_header())
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "Springtale")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .json(&serde_json::json!({
                "title": title,
                "body": body,
            }))
            .send()
            .await?;

        handle_json_response(response)
            .await
            .map_err(GithubError::RequestFailed)
    }

    /// Post a comment on an issue or pull request.
    async fn post_comment(
        &self,
        owner: &str,
        repo: &str,
        issue_number: u64,
        body: &str,
    ) -> Result<serde_json::Value, GithubError> {
        let url = format!(
            "{}/repos/{owner}/{repo}/issues/{issue_number}/comments",
            self.api_base
        );

        let response = self
            .inner
            .post(&url)
            .header("Authorization", self.auth_header())
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "Springtale")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .json(&serde_json::json!({
                "body": body,
            }))
            .send()
            .await?;

        handle_json_response(response)
            .await
            .map_err(GithubError::RequestFailed)
    }

    /// Get the diff for a pull request.
    async fn get_diff(
        &self,
        owner: &str,
        repo: &str,
        pull_number: u64,
    ) -> Result<String, GithubError> {
        let url = format!(
            "{}/repos/{owner}/{repo}/pulls/{pull_number}",
            self.api_base
        );

        let response = self
            .inner
            .get(&url)
            .header("Authorization", self.auth_header())
            .header("Accept", "application/vnd.github.diff")
            .header("User-Agent", "Springtale")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_owned());
            return Err(GithubError::RequestFailed(format!(
                "GitHub API returned {status}: {body}"
            )));
        }

        response
            .text()
            .await
            .map_err(|e| GithubError::RequestFailed(format!("failed to read diff: {e}")))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use secrecy::SecretBox;

    #[test]
    fn test_client_creation() {
        let config = GithubConfig {
            token: SecretBox::new(Box::new("ghp_test".to_owned())),
            webhook_secret: None,
            api_base: "https://api.github.com".to_owned(),
        };
        let client = GithubClient::new(&config);
        assert!(client.is_ok());
    }
}
