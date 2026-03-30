use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretBox};

use crate::error::KickError;

/// Trait defining the Kick API surface used by actions.
///
/// Actions depend on this trait, not the concrete client. This enables
/// mock implementations in tests (per testing.md: "mock at the client
/// layer, not at reqwest level").
#[async_trait]
pub trait KickApi: Send + Sync {
    async fn send_chat(&self, channel_id: &str, message: &str) -> Result<serde_json::Value, KickError>;
    async fn get_channel_by_slug(&self, slug: &str) -> Result<serde_json::Value, KickError>;
    async fn get_stream(&self, channel_id: &str) -> Result<serde_json::Value, KickError>;
}

/// Kick REST API client.
///
/// All API calls to Kick go through this client. It manages the access
/// token and required headers per Kick's API documentation.
pub struct KickClient {
    inner: reqwest::Client,
    api_base: String,
    access_token: SecretBox<String>,
}

impl KickClient {
    /// Create a new Kick API client with the given access token.
    pub fn new(api_base: &str, access_token: SecretBox<String>) -> Result<Self, KickError> {
        let inner = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| KickError::RequestFailed(format!("failed to build client: {e}")))?;

        Ok(Self {
            inner,
            api_base: api_base.to_owned(),
            access_token,
        })
    }

    /// Exchange an authorization code for an access token.
    ///
    /// All reqwest calls must go through client/ — this is the HTTP call
    /// that `auth::exchange_code` delegates to.
    pub async fn exchange_token(
        oauth_base: &str,
        form_body: String,
    ) -> Result<String, KickError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| KickError::AuthFailed(format!("failed to build client: {e}")))?;

        let response = client
            .post(format!("{oauth_base}/oauth/token"))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(form_body)
            .send()
            .await
            .map_err(|e| KickError::AuthFailed(format!("token request failed: {e}")))?;

        let status = response.status().as_u16();
        let body = response
            .text()
            .await
            .map_err(|e| KickError::AuthFailed(format!("failed to read token response: {e}")))?;

        if status >= 400 {
            return Err(KickError::AuthFailed(format!(
                "token exchange failed ({status}): {body}"
            )));
        }

        Ok(body)
    }

    /// Subscribe to webhook events.
    /// `POST /public/v1/events/subscriptions`
    ///
    /// Per Kick docs, the body includes the event type and callback URL.
    pub async fn subscribe_events(
        &self,
        event_types: &[&str],
        callback_url: &str,
    ) -> Result<serde_json::Value, KickError> {
        let url = format!("{}/public/v1/events/subscriptions", self.api_base);

        // Subscribe to each event type
        let mut results = Vec::new();
        for event_type in event_types {
            let response = self
                .inner
                .post(&url)
                // SECURITY: expose needed for Bearer auth on API call
                .bearer_auth(self.access_token.expose_secret())
                .header("Accept", "application/json")
                .json(&serde_json::json!({
                    "type": event_type,
                    "version": 1,
                    "method": "webhook",
                    "callback": callback_url,
                }))
                .send()
                .await?;

            results.push(handle_response(response).await?);
        }

        Ok(serde_json::json!(results))
    }

    /// Fetch the Kick public key for webhook signature verification.
    /// `GET /public/v1/public-key`
    ///
    /// Returns the PEM-encoded RSA public key.
    pub async fn fetch_public_key(api_base: &str) -> Result<String, KickError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| KickError::RequestFailed(format!("failed to build client: {e}")))?;

        let url = format!("{api_base}/public/v1/public-key");
        let response = client
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| KickError::RequestFailed(format!("failed to fetch public key: {e}")))?;

        let status = response.status().as_u16();
        let body = response
            .text()
            .await
            .map_err(|e| KickError::RequestFailed(format!("failed to read public key: {e}")))?;

        if status >= 400 {
            return Err(KickError::RequestFailed(format!(
                "public key fetch failed ({status}): {body}"
            )));
        }

        // The response may be JSON with a "data" field containing the PEM key,
        // or it may be the PEM directly. Handle both.
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body)
            && let Some(key) = json.get("data").and_then(|d| d.as_str())
        {
            return Ok(key.to_owned());
        }

        Ok(body)
    }
}

#[async_trait]
impl KickApi for KickClient {
    /// Send a chat message to a channel.
    /// `POST /public/v1/chat`
    async fn send_chat(
        &self,
        channel_id: &str,
        message: &str,
    ) -> Result<serde_json::Value, KickError> {
        let url = format!("{}/public/v1/chat", self.api_base);

        let response = self
            .inner
            .post(&url)
            // SECURITY: expose needed for Bearer auth on API call
            .bearer_auth(self.access_token.expose_secret())
            .header("Accept", "application/json")
            .json(&serde_json::json!({
                "channel_id": channel_id,
                "content": message,
            }))
            .send()
            .await?;

        handle_response(response).await
    }

    /// Get channel information.
    /// `GET /public/v1/channels?slug={slug}` or `?broadcaster_user_id={id}`
    /// Kick API supports both query params (slug added 08/04/2025).
    async fn get_channel_by_slug(
        &self,
        slug: &str,
    ) -> Result<serde_json::Value, KickError> {
        let url = format!(
            "{}/public/v1/channels?slug={slug}",
            self.api_base
        );

        let response = self
            .inner
            .get(&url)
            // SECURITY: expose needed for Bearer auth on API call
            .bearer_auth(self.access_token.expose_secret())
            .header("Accept", "application/json")
            .send()
            .await?;

        handle_response(response).await
    }

    /// Get livestream status.
    /// `GET /public/v1/livestreams?channel_id={id}`
    async fn get_stream(
        &self,
        channel_id: &str,
    ) -> Result<serde_json::Value, KickError> {
        let url = format!(
            "{}/public/v1/livestreams?channel_id={channel_id}",
            self.api_base
        );

        let response = self
            .inner
            .get(&url)
            // SECURITY: expose needed for Bearer auth on API call
            .bearer_auth(self.access_token.expose_secret())
            .header("Accept", "application/json")
            .send()
            .await?;

        handle_response(response).await
    }
}

/// Handle a JSON API response.
async fn handle_response(response: reqwest::Response) -> Result<serde_json::Value, KickError> {
    let status = response.status().as_u16();
    let body = response
        .text()
        .await
        .map_err(|e| KickError::RequestFailed(format!("failed to read response: {e}")))?;

    if status >= 400 {
        return Err(KickError::RequestFailed(format!(
            "Kick API returned {status}: {body}"
        )));
    }

    serde_json::from_str(&body)
        .map_err(|e| KickError::RequestFailed(format!("failed to parse response: {e}")))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = KickClient::new(
            "https://api.kick.com",
            SecretBox::new(Box::new("test_token".to_owned())),
        );
        assert!(client.is_ok());
    }
}
