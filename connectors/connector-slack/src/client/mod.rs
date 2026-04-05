use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretBox};

use crate::error::SlackError;

/// Trait defining the Slack API surface used by actions.
/// Actions depend on this trait — enables mock testing.
#[async_trait]
pub trait SlackApi: Send + Sync {
    /// Send a text message to a channel.
    async fn send_message(
        &self,
        channel: &str,
        text: &str,
    ) -> Result<serde_json::Value, SlackError>;

    /// Send a Block Kit message to a channel.
    async fn send_blocks(
        &self,
        channel: &str,
        blocks: serde_json::Value,
    ) -> Result<serde_json::Value, SlackError>;

    /// Send a thread reply.
    async fn send_thread_reply(
        &self,
        channel: &str,
        thread_ts: &str,
        text: &str,
        broadcast: bool,
    ) -> Result<serde_json::Value, SlackError>;

    /// Edit an existing message.
    async fn edit_message(&self, channel: &str, ts: &str, text: &str) -> Result<(), SlackError>;

    /// Add a reaction to a message.
    async fn add_reaction(
        &self,
        channel: &str,
        timestamp: &str,
        name: &str,
    ) -> Result<(), SlackError>;
}

/// Concrete Slack client wrapping reqwest.
///
/// All API calls go to `https://slack.com/api/{method}` with
/// `Authorization: Bearer xoxb-...` header.
///
/// Applies publish-side jitter before every outbound call.
///
/// IMPORTANT: Slack returns HTTP 200 even on errors — must check
/// the `ok` field in every response body.
pub struct SlackClient {
    http: reqwest::Client,
    base_url: String,
    bot_token: SecretBox<String>,
    jitter_secs: u64,
}

impl SlackClient {
    /// Create a new SlackClient.
    ///
    /// `bot_token` stays wrapped in `SecretBox` — only exposed at the
    /// precise HTTP call site (Authorization header).
    pub fn new(bot_token: SecretBox<String>, jitter_secs: u64) -> Self {
        let http = reqwest::Client::new();
        Self {
            http,
            base_url: "https://slack.com/api".to_owned(),
            bot_token,
            jitter_secs,
        }
    }

    /// Apply publish-side jitter before sending.
    async fn apply_jitter(&self) {
        if self.jitter_secs > 0 {
            let jitter = rand::random::<u64>() % self.jitter_secs;
            tokio::time::sleep(std::time::Duration::from_secs(jitter)).await;
        }
    }

    /// POST to a Slack API method and check the `ok` field.
    async fn api_post(
        &self,
        method: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, SlackError> {
        let url = format!("{}/{method}", self.base_url);
        let response = self
            .http
            .post(&url)
            // SECURITY: expose needed for Slack API Bearer auth
            .header(
                "Authorization",
                format!("Bearer {}", self.bot_token.expose_secret()),
            )
            .header("Content-Type", "application/json; charset=utf-8")
            .json(&body)
            .send()
            .await
            .map_err(|e| SlackError::SendFailed(format!("{method} request failed: {e}")))?;

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| SlackError::ApiError(format!("{method} response parse failed: {e}")))?;

        // Slack returns 200 even on errors — check ok field
        if json.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            let error = json
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("unknown");
            return Err(SlackError::ApiError(format!("{method}: {error}")));
        }

        Ok(json)
    }
}

#[async_trait]
impl SlackApi for SlackClient {
    async fn send_message(
        &self,
        channel: &str,
        text: &str,
    ) -> Result<serde_json::Value, SlackError> {
        self.apply_jitter().await;
        let body = serde_json::json!({
            "channel": channel,
            "text": text,
        });
        self.api_post("chat.postMessage", body).await
    }

    async fn send_blocks(
        &self,
        channel: &str,
        blocks: serde_json::Value,
    ) -> Result<serde_json::Value, SlackError> {
        self.apply_jitter().await;
        let body = serde_json::json!({
            "channel": channel,
            "blocks": blocks,
        });
        self.api_post("chat.postMessage", body).await
    }

    async fn send_thread_reply(
        &self,
        channel: &str,
        thread_ts: &str,
        text: &str,
        broadcast: bool,
    ) -> Result<serde_json::Value, SlackError> {
        self.apply_jitter().await;
        let body = serde_json::json!({
            "channel": channel,
            "thread_ts": thread_ts,
            "text": text,
            "reply_broadcast": broadcast,
        });
        self.api_post("chat.postMessage", body).await
    }

    async fn edit_message(&self, channel: &str, ts: &str, text: &str) -> Result<(), SlackError> {
        self.apply_jitter().await;
        let body = serde_json::json!({
            "channel": channel,
            "ts": ts,
            "text": text,
        });
        self.api_post("chat.update", body).await?;
        Ok(())
    }

    async fn add_reaction(
        &self,
        channel: &str,
        timestamp: &str,
        name: &str,
    ) -> Result<(), SlackError> {
        self.apply_jitter().await;
        let body = serde_json::json!({
            "channel": channel,
            "timestamp": timestamp,
            "name": name,
        });
        self.api_post("reactions.add", body).await?;
        Ok(())
    }
}

#[cfg(test)]
pub mod test_helpers {
    use super::*;

    pub struct MockSlackApi;

    #[async_trait]
    impl SlackApi for MockSlackApi {
        async fn send_message(
            &self,
            channel: &str,
            content: &str,
        ) -> Result<serde_json::Value, SlackError> {
            Ok(serde_json::json!({
                "ok": true,
                "channel": channel,
                "ts": "1234567890.123456",
                "message": { "text": content },
            }))
        }

        async fn send_blocks(
            &self,
            channel: &str,
            _blocks: serde_json::Value,
        ) -> Result<serde_json::Value, SlackError> {
            Ok(serde_json::json!({
                "ok": true,
                "channel": channel,
                "ts": "1234567890.123456",
            }))
        }

        async fn send_thread_reply(
            &self,
            channel: &str,
            _thread_ts: &str,
            _text: &str,
            _broadcast: bool,
        ) -> Result<serde_json::Value, SlackError> {
            Ok(serde_json::json!({
                "ok": true,
                "channel": channel,
                "ts": "1234567890.654321",
            }))
        }

        async fn edit_message(
            &self,
            _channel: &str,
            _ts: &str,
            _text: &str,
        ) -> Result<(), SlackError> {
            Ok(())
        }

        async fn add_reaction(
            &self,
            _channel: &str,
            _timestamp: &str,
            _name: &str,
        ) -> Result<(), SlackError> {
            Ok(())
        }
    }
}
