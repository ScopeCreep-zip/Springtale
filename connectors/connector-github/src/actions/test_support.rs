//! Shared test doubles for action unit tests. One mock implements the full
//! `GithubApi` surface so the trait can grow without touching every test
//! file. Returns a single canned JSON for every call (string for `get_diff`).
//!
//! `#[cfg(test)]` is applied at the module declaration in `actions/mod.rs`.

use crate::client::GithubApi;
use crate::error::GithubError;

pub struct MockGithubClient {
    pub response: serde_json::Value,
}

impl MockGithubClient {
    pub fn new(response: serde_json::Value) -> Self {
        Self { response }
    }
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

    async fn get_ref_sha(
        &self,
        _owner: &str,
        _repo: &str,
        _branch: &str,
    ) -> Result<String, GithubError> {
        self.response
            .pointer("/object/sha")
            .and_then(|s| s.as_str())
            .map(str::to_owned)
            .ok_or_else(|| GithubError::RequestFailed("mock missing object.sha".into()))
    }

    async fn create_branch(
        &self,
        _owner: &str,
        _repo: &str,
        _branch: &str,
        _sha: &str,
    ) -> Result<serde_json::Value, GithubError> {
        Ok(self.response.clone())
    }

    async fn commit_file(
        &self,
        _owner: &str,
        _repo: &str,
        _branch: &str,
        _path: &str,
        _content_b64: &str,
        _message: &str,
        _existing_sha: Option<&str>,
    ) -> Result<serde_json::Value, GithubError> {
        Ok(self.response.clone())
    }

    async fn create_pr(
        &self,
        _owner: &str,
        _repo: &str,
        _title: &str,
        _head: &str,
        _base: &str,
        _body: &str,
    ) -> Result<serde_json::Value, GithubError> {
        Ok(self.response.clone())
    }
}
