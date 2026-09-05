use std::time::Duration;

use async_trait::async_trait;
use octocrab::Octocrab;
use octocrab::models::repos::Object;
use octocrab::params::repos::Reference;
use secrecy::SecretString;

use crate::config::GithubConfig;
use crate::error::GithubError;

/// Trait defining the GitHub API surface.
///
/// Actions depend on this trait, not the concrete client. This enables
/// mock implementations in tests (per testing.md: "mock at the client
/// layer, not at reqwest level"). Responses are the SDK's typed models
/// re-encoded as JSON so actions and their mocks share one shape.
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

    /// Create or update one file on a branch (`PUT /contents/{path}`).
    /// `content` is the raw file bytes; the SDK base64-encodes them.
    async fn commit_file(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
        path: &str,
        content: &[u8],
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

/// GitHub REST API client backed by [`octocrab`].
///
/// Transport is hyper + rustls (ring) — the workspace's `native-tls` /
/// `openssl` stubs guarantee no other TLS stack can be linked.
pub struct GithubClient {
    inner: Octocrab,
}

impl GithubClient {
    /// Create a new GitHub API client from config.
    ///
    /// Must be called inside a Tokio runtime: octocrab spawns its request
    /// buffer worker at build time (the factory's `create` is `async`).
    pub fn new(config: &GithubConfig) -> Result<Self, GithubError> {
        // SECURITY: expose needed to hand the PAT to octocrab, which keeps it
        // as a `SecretString`; the plaintext never outlives this closure.
        let token = springtale_crypto::secret_use::with_str(&config.token, |t| {
            SecretString::from(t.to_owned())
        });

        let inner = Octocrab::builder()
            .personal_token(token)
            .base_uri(config.api_base.as_str())?
            .set_connect_timeout(Some(Duration::from_secs(5)))
            .set_read_timeout(Some(Duration::from_secs(30)))
            .set_write_timeout(Some(Duration::from_secs(30)))
            .build()?;

        Ok(Self { inner })
    }
}

/// Re-encode a typed SDK model as JSON for the trait boundary.
fn to_json<T: serde::Serialize>(value: T) -> Result<serde_json::Value, GithubError> {
    serde_json::to_value(value)
        .map_err(|e| GithubError::RequestFailed(format!("failed to encode response: {e}")))
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
        let issue = self
            .inner
            .issues(owner, repo)
            .create(title)
            .body(body)
            .send()
            .await?;
        to_json(issue)
    }

    /// Post a comment on an issue or pull request.
    async fn post_comment(
        &self,
        owner: &str,
        repo: &str,
        issue_number: u64,
        body: &str,
    ) -> Result<serde_json::Value, GithubError> {
        let comment = self
            .inner
            .issues(owner, repo)
            .create_comment(issue_number, body)
            .await?;
        to_json(comment)
    }

    /// Get the diff for a pull request.
    async fn get_diff(
        &self,
        owner: &str,
        repo: &str,
        pull_number: u64,
    ) -> Result<String, GithubError> {
        Ok(self.inner.pulls(owner, repo).get_diff(pull_number).await?)
    }

    async fn get_ref_sha(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
    ) -> Result<String, GithubError> {
        let git_ref = self
            .inner
            .repos(owner, repo)
            .get_ref(&Reference::Branch(branch.to_owned()))
            .await?;
        match git_ref.object {
            Object::Commit { sha, .. } => Ok(sha),
            _ => Err(GithubError::UnexpectedRef),
        }
    }

    async fn create_branch(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
        sha: &str,
    ) -> Result<serde_json::Value, GithubError> {
        let created = self
            .inner
            .repos(owner, repo)
            .create_ref(&Reference::Branch(branch.to_owned()), sha)
            .await?;
        to_json(created)
    }

    async fn commit_file(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
        path: &str,
        content: &[u8],
        message: &str,
        existing_sha: Option<&str>,
    ) -> Result<serde_json::Value, GithubError> {
        let repos = self.inner.repos(owner, repo);
        let request = match existing_sha {
            Some(sha) => repos.update_file(path, message, content, sha),
            None => repos.create_file(path, message, content),
        };
        let updated = request.branch(branch).send().await?;
        to_json(updated)
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
        let pr = self
            .inner
            .pulls(owner, repo)
            .create(title, head, base)
            .body(body)
            .send()
            .await?;
        to_json(pr)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use secrecy::SecretBox;

    #[tokio::test]
    async fn test_client_creation() {
        let config = GithubConfig {
            token: SecretBox::new(Box::new("ghp_test".to_owned())),
            webhook_secret: None,
            api_base: "https://api.github.com".to_owned(),
        };
        let client = GithubClient::new(&config);
        assert!(client.is_ok());
    }
}
