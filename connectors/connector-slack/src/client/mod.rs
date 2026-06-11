use async_trait::async_trait;
use secrecy::SecretBox;

use crate::error::SlackError;

/// A Slack conversation discovered via active enumeration.
///
/// Surfaces the minimum metadata needed to build a
/// workspace key. The Slack ID's first character determines the kind
/// (`C` channel, `G` private/legacy channel, `D` IM, `M` mpim).
#[derive(Debug, Clone)]
pub struct DiscoveredSlackConversation {
    pub id: String,
    pub name: Option<String>,
    pub is_im: bool,
    pub is_mpim: bool,
    pub is_private: bool,
    pub num_members: Option<u64>,
}

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

    /// Enumerate every conversation this bot has access to —
    /// public + private channels, IMs, and mpims via cursor-paginated
    /// `conversations.list`.
    async fn list_destinations(&self) -> Result<Vec<DiscoveredSlackConversation>, SlackError>;
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
        // safe_http::client() gives us rustls + PQ KEX + 30s timeout +
        // limited redirects. Falls back to a fresh client on factory
        // failure so the Slack connector still constructs even if the
        // rustls provider has not been installed yet (unit tests).
        let http = springtale_transport::safe_http::client().unwrap_or_default();
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
            .header(
                "Authorization",
                springtale_crypto::secret_use::bearer_header(&self.bot_token),
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

    async fn list_destinations(&self) -> Result<Vec<DiscoveredSlackConversation>, SlackError> {
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut body = serde_json::json!({
                "types": "public_channel,private_channel,mpim,im",
                "limit": 200,
                "exclude_archived": true,
            });
            if let Some(c) = &cursor {
                body["cursor"] = serde_json::Value::String(c.clone());
            }
            let resp = self.api_post("conversations.list", body).await?;
            if let Some(channels) = resp.get("channels").and_then(|c| c.as_array()) {
                for ch in channels {
                    let id = match ch.get("id").and_then(|v| v.as_str()) {
                        Some(s) => s.to_owned(),
                        None => continue,
                    };
                    out.push(DiscoveredSlackConversation {
                        id,
                        name: ch.get("name").and_then(|v| v.as_str()).map(str::to_owned),
                        is_im: ch.get("is_im").and_then(|v| v.as_bool()).unwrap_or(false),
                        is_mpim: ch.get("is_mpim").and_then(|v| v.as_bool()).unwrap_or(false),
                        is_private: ch
                            .get("is_private")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false),
                        num_members: ch.get("num_members").and_then(|v| v.as_u64()),
                    });
                }
            }
            cursor = resp
                .get("response_metadata")
                .and_then(|m| m.get("next_cursor"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_owned);
            if cursor.is_none() {
                break;
            }
        }
        Ok(out)
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

        async fn list_destinations(&self) -> Result<Vec<DiscoveredSlackConversation>, SlackError> {
            // Synthetic mix covering every prefix path the URI scheme
            // distinguishes: C (public channel), G (private channel),
            // D (IM), M (mpim).
            Ok(vec![
                DiscoveredSlackConversation {
                    id: "C0PUBLIC1".to_owned(),
                    name: Some("general".to_owned()),
                    is_im: false,
                    is_mpim: false,
                    is_private: false,
                    num_members: Some(42),
                },
                DiscoveredSlackConversation {
                    id: "C0PUBLIC2".to_owned(),
                    name: Some("random".to_owned()),
                    is_im: false,
                    is_mpim: false,
                    is_private: false,
                    num_members: Some(35),
                },
                DiscoveredSlackConversation {
                    id: "G0PRIV1".to_owned(),
                    name: Some("team-private".to_owned()),
                    is_im: false,
                    is_mpim: false,
                    is_private: true,
                    num_members: Some(8),
                },
                DiscoveredSlackConversation {
                    id: "D0DM1".to_owned(),
                    name: None,
                    is_im: true,
                    is_mpim: false,
                    is_private: true,
                    num_members: Some(2),
                },
                DiscoveredSlackConversation {
                    id: "M0MPIM1".to_owned(),
                    name: None,
                    is_im: false,
                    is_mpim: true,
                    is_private: true,
                    num_members: Some(4),
                },
            ])
        }
    }
}
