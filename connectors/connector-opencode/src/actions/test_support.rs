//! Shared test double for action unit tests.

#![cfg(test)]

use async_trait::async_trait;

use crate::client::OpenCodeApi;
use crate::error::OpenCodeError;

pub struct MockOpenCodeClient {
    session_id: String,
    message_response: serde_json::Value,
}

impl MockOpenCodeClient {
    pub fn new(session_id: &str, message_response: serde_json::Value) -> Self {
        Self {
            session_id: session_id.to_owned(),
            message_response,
        }
    }
}

#[async_trait]
impl OpenCodeApi for MockOpenCodeClient {
    async fn create_session(&self, _title: Option<&str>) -> Result<String, OpenCodeError> {
        Ok(self.session_id.clone())
    }

    async fn send_prompt(
        &self,
        _session_id: &str,
        _prompt: &str,
    ) -> Result<serde_json::Value, OpenCodeError> {
        Ok(self.message_response.clone())
    }
}
