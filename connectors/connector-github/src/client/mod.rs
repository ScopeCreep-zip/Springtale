use async_trait::async_trait;
use secrecy::SecretBox;
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

    // ── W4 code-change parity: the create-PR flow ──────────────

    /// SHA of a branch head (`GET /git/ref/heads/{branch}`) — the base
    /// for `create_branch`.
    async fn get_ref_sha(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
    ) -> Result<String, GithubError>;

    /// Create a branch at `sha` (`POST /git/refs`).
    async fn create_branch(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
        sha: &str,
    ) -> Result<serde_json::Value, GithubError>;

    /// Create or update one file on a branch
    /// (`PUT /contents/{path}`, content base64-encoded by the caller).
    async fn commit_file(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
        path: &str,
        content_b64: &str,
        message: &str,
        existing_sha: Option<&str>,
    ) -> Result<serde_json::Value, GithubError>;

    /// Open a pull request (`POST /pulls`).
    async fn create_pr(
        &self,
        owner: &str,
        repo: &str,
        title: &str,
        head: &str,
        base: &str,
        body: &str,
    ) -> Result<serde_json::Value, GithubError>;
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
        let inner = springtale_transport::safe_http::builder()
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
        springtale_crypto::secret_use::bearer_header(&self.auth_token)
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
        let url = format!("{}/repos/{owner}/{repo}/pulls/{pull_number}", self.api_base);

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

    async fn get_ref_sha(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
    ) -> Result<String, GithubError> {
        let url = format!(
            "{}/repos/{owner}/{repo}/git/ref/heads/{branch}",
            self.api_base
        );
        let response = self
            .inner
            .get(&url)
            .header("Authorization", self.auth_header())
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "Springtale")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await?;
        let json = handle_json_response(response)
            .await
            .map_err(GithubError::RequestFailed)?;
        json.pointer("/object/sha")
            .and_then(|s| s.as_str())
            .map(str::to_owned)
            .ok_or_else(|| GithubError::RequestFailed("ref response missing object.sha".into()))
    }

    async fn create_branch(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
        sha: &str,
    ) -> Result<serde_json::Value, GithubError> {
        let url = format!("{}/repos/{owner}/{repo}/git/refs", self.api_base);
        let response = self
            .inner
            .post(&url)
            .header("Authorization", self.auth_header())
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "Springtale")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .json(&serde_json::json!({
                "ref": format!("refs/heads/{branch}"),
                "sha": sha,
            }))
            .send()
            .await?;
        handle_json_response(response)
            .await
            .map_err(GithubError::RequestFailed)
    }

    async fn commit_file(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
        path: &str,
        content_b64: &str,
        message: &str,
        existing_sha: Option<&str>,
    ) -> Result<serde_json::Value, GithubError> {
        let url = format!("{}/repos/{owner}/{repo}/contents/{path}", self.api_base);
        let mut body = serde_json::json!({
            "message": message,
            "content": content_b64,
            "branch": branch,
        });
        if let (Some(sha), Some(obj)) = (existing_sha, body.as_object_mut()) {
            obj.insert("sha".into(), serde_json::Value::String(sha.to_owned()));
        }
        let response = self
            .inner
            .put(&url)
            .header("Authorization", self.auth_header())
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "Springtale")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .json(&body)
            .send()
            .await?;
        handle_json_response(response)
            .await
            .map_err(GithubError::RequestFailed)
    }

    async fn create_pr(
        &self,
        owner: &str,
        repo: &str,
        title: &str,
        head: &str,
        base: &str,
        body: &str,
    ) -> Result<serde_json::Value, GithubError> {
        let url = format!("{}/repos/{owner}/{repo}/pulls", self.api_base);
        let response = self
            .inner
            .post(&url)
            .header("Authorization", self.auth_header())
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "Springtale")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .json(&serde_json::json!({
                "title": title,
                "head": head,
                "base": base,
                "body": body,
            }))
            .send()
            .await?;
        handle_json_response(response)
            .await
            .map_err(GithubError::RequestFailed)
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
