use async_trait::async_trait;
use secrecy::SecretBox;

use crate::error::TelegramError;

/// Trait defining the Telegram Bot API surface used by actions.
/// Actions depend on trait, not concrete client — enables mock testing.
#[async_trait]
pub trait TelegramApi: Send + Sync {
    async fn send_message(
        &self,
        chat_id: &str,
        text: &str,
        parse_mode: Option<&str>,
        reply_to_message_id: Option<i64>,
    ) -> Result<serde_json::Value, TelegramError>;

    async fn send_photo(
        &self,
        chat_id: &str,
        photo: &str,
        caption: Option<&str>,
    ) -> Result<serde_json::Value, TelegramError>;

    async fn edit_message_text(
        &self,
        chat_id: &str,
        message_id: i64,
        text: &str,
        parse_mode: Option<&str>,
    ) -> Result<serde_json::Value, TelegramError>;

    async fn delete_message(
        &self,
        chat_id: &str,
        message_id: i64,
    ) -> Result<serde_json::Value, TelegramError>;

    async fn send_inline_keyboard(
        &self,
        chat_id: &str,
        text: &str,
        inline_keyboard: serde_json::Value,
    ) -> Result<serde_json::Value, TelegramError>;

    async fn get_updates(
        &self,
        offset: Option<i64>,
        timeout: u64,
        allowed_updates: &[String],
    ) -> Result<serde_json::Value, TelegramError>;

    async fn set_webhook(
        &self,
        url: &str,
        secret_token: Option<&str>,
        allowed_updates: &[String],
    ) -> Result<serde_json::Value, TelegramError>;

    async fn delete_webhook(&self) -> Result<serde_json::Value, TelegramError>;

    async fn get_me(&self) -> Result<serde_json::Value, TelegramError>;

    /// Answer a callback_query (inline keyboard button press).
    ///
    /// MUST be called within 10 seconds of receiving the callback_query
    /// or Telegram will show a perpetual loading spinner on the button.
    /// Pass empty text to dismiss silently; set `show_alert=true` for a
    /// modal popup instead of a toast notification.
    async fn answer_callback_query(
        &self,
        callback_query_id: &str,
        text: Option<&str>,
        show_alert: bool,
    ) -> Result<serde_json::Value, TelegramError>;
}

/// Telegram Bot API REST client.
/// All network calls to Telegram go through this client.
pub struct TelegramClient {
    inner: reqwest::Client,
    api_base: String,
    bot_token: SecretBox<String>,
}

impl TelegramClient {
    pub fn new(api_base: &str, bot_token: SecretBox<String>) -> Result<Self, TelegramError> {
        let inner = springtale_transport::safe_http::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| TelegramError::RequestFailed(format!("failed to build client: {e}")))?;

        Ok(Self {
            inner,
            api_base: api_base.to_owned(),
            bot_token,
        })
    }

    /// Build the full Bot API URL for a method. The token sits in the
    /// URL path per Telegram's Bot API; exposed only inside the closure.
    fn method_url(&self, method: &str) -> String {
        springtale_crypto::secret_use::with_str(&self.bot_token, |tok| {
            format!("{}/bot{}/{}", self.api_base, tok, method)
        })
    }
}

/// Parse Telegram Bot API response, extracting the "result" field.
/// Telegram returns `{"ok": true, "result": ...}` or `{"ok": false, "description": "..."}`.
async fn handle_telegram_response(
    response: reqwest::Response,
) -> Result<serde_json::Value, TelegramError> {
    let status = response.status().as_u16();
    let body = response
        .text()
        .await
        .map_err(|e| TelegramError::RequestFailed(format!("failed to read response: {e}")))?;

    let json: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| TelegramError::RequestFailed(format!("invalid JSON: {e}")))?;

    if status == 429 {
        let retry_after = json
            .get("parameters")
            .and_then(|p| p.get("retry_after"))
            .and_then(|r| r.as_u64())
            .unwrap_or(5);
        return Err(TelegramError::RateLimited { retry_after });
    }

    if status >= 400 {
        let desc = json
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or("unknown error");
        return Err(TelegramError::RequestFailed(format!(
            "API returned {status}: {desc}"
        )));
    }

    // Telegram can return HTTP 200 with ok:false in the JSON body.
    let ok = json.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
    if !ok {
        let desc = json
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or("unknown error");
        return Err(TelegramError::RequestFailed(format!("API error: {desc}")));
    }

    json.get("result")
        .cloned()
        .ok_or_else(|| TelegramError::RequestFailed("missing 'result' in response".to_owned()))
}

#[async_trait]
impl TelegramApi for TelegramClient {
    async fn send_message(
        &self,
        chat_id: &str,
        text: &str,
        parse_mode: Option<&str>,
        reply_to_message_id: Option<i64>,
    ) -> Result<serde_json::Value, TelegramError> {
        let mut body = serde_json::json!({
            "chat_id": chat_id,
            "text": text,
        });
        if let Some(pm) = parse_mode {
            body["parse_mode"] = serde_json::Value::String(pm.to_owned());
        }
        if let Some(reply_id) = reply_to_message_id {
            body["reply_to_message_id"] = serde_json::Value::Number(reply_id.into());
        }

        let response = self
            .inner
            .post(self.method_url("sendMessage"))
            .json(&body)
            .send()
            .await?;
        handle_telegram_response(response).await
    }

    async fn send_photo(
        &self,
        chat_id: &str,
        photo: &str,
        caption: Option<&str>,
    ) -> Result<serde_json::Value, TelegramError> {
        let mut body = serde_json::json!({
            "chat_id": chat_id,
            "photo": photo,
        });
        if let Some(cap) = caption {
            body["caption"] = serde_json::Value::String(cap.to_owned());
        }

        let response = self
            .inner
            .post(self.method_url("sendPhoto"))
            .json(&body)
            .send()
            .await?;
        handle_telegram_response(response).await
    }

    async fn edit_message_text(
        &self,
        chat_id: &str,
        message_id: i64,
        text: &str,
        parse_mode: Option<&str>,
    ) -> Result<serde_json::Value, TelegramError> {
        let mut body = serde_json::json!({
            "chat_id": chat_id,
            "message_id": message_id,
            "text": text,
        });
        if let Some(pm) = parse_mode {
            body["parse_mode"] = serde_json::Value::String(pm.to_owned());
        }

        let response = self
            .inner
            .post(self.method_url("editMessageText"))
            .json(&body)
            .send()
            .await?;
        handle_telegram_response(response).await
    }

    async fn delete_message(
        &self,
        chat_id: &str,
        message_id: i64,
    ) -> Result<serde_json::Value, TelegramError> {
        let body = serde_json::json!({
            "chat_id": chat_id,
            "message_id": message_id,
        });

        let response = self
            .inner
            .post(self.method_url("deleteMessage"))
            .json(&body)
            .send()
            .await?;
        handle_telegram_response(response).await
    }

    async fn send_inline_keyboard(
        &self,
        chat_id: &str,
        text: &str,
        inline_keyboard: serde_json::Value,
    ) -> Result<serde_json::Value, TelegramError> {
        let body = serde_json::json!({
            "chat_id": chat_id,
            "text": text,
            "reply_markup": {
                "inline_keyboard": inline_keyboard,
            },
        });

        let response = self
            .inner
            .post(self.method_url("sendMessage"))
            .json(&body)
            .send()
            .await?;
        handle_telegram_response(response).await
    }

    async fn get_updates(
        &self,
        offset: Option<i64>,
        timeout: u64,
        allowed_updates: &[String],
    ) -> Result<serde_json::Value, TelegramError> {
        let mut body = serde_json::json!({
            "timeout": timeout,
        });
        if let Some(off) = offset {
            body["offset"] = serde_json::Value::Number(off.into());
        }
        if !allowed_updates.is_empty() {
            body["allowed_updates"] = serde_json::to_value(allowed_updates)
                .unwrap_or_else(|_| serde_json::Value::Array(vec![]));
        }

        let response = self
            .inner
            .post(self.method_url("getUpdates"))
            .json(&body)
            .send()
            .await?;
        handle_telegram_response(response).await
    }

    async fn set_webhook(
        &self,
        url: &str,
        secret_token: Option<&str>,
        allowed_updates: &[String],
    ) -> Result<serde_json::Value, TelegramError> {
        let mut body = serde_json::json!({
            "url": url,
        });
        if let Some(token) = secret_token {
            body["secret_token"] = serde_json::Value::String(token.to_owned());
        }
        if !allowed_updates.is_empty() {
            body["allowed_updates"] = serde_json::to_value(allowed_updates)
                .unwrap_or_else(|_| serde_json::Value::Array(vec![]));
        }

        let response = self
            .inner
            .post(self.method_url("setWebhook"))
            .json(&body)
            .send()
            .await?;
        handle_telegram_response(response).await
    }

    async fn delete_webhook(&self) -> Result<serde_json::Value, TelegramError> {
        let response = self
            .inner
            .post(self.method_url("deleteWebhook"))
            .send()
            .await?;
        handle_telegram_response(response).await
    }

    async fn get_me(&self) -> Result<serde_json::Value, TelegramError> {
        let response = self.inner.get(self.method_url("getMe")).send().await?;
        handle_telegram_response(response).await
    }

    async fn answer_callback_query(
        &self,
        callback_query_id: &str,
        text: Option<&str>,
        show_alert: bool,
    ) -> Result<serde_json::Value, TelegramError> {
        let mut body = serde_json::json!({
            "callback_query_id": callback_query_id,
            "show_alert": show_alert,
        });
        if let Some(t) = text {
            body["text"] = serde_json::Value::String(t.to_owned());
        }
        let response = self
            .inner
            .post(self.method_url("answerCallbackQuery"))
            .json(&body)
            .send()
            .await?;
        handle_telegram_response(response).await
    }
}

#[cfg(test)]
pub mod test_helpers {
    use super::*;

    pub struct MockTelegramApi {
        pub response: serde_json::Value,
    }

    #[async_trait]
    impl TelegramApi for MockTelegramApi {
        async fn send_message(
            &self,
            _: &str,
            _: &str,
            _: Option<&str>,
            _: Option<i64>,
        ) -> Result<serde_json::Value, TelegramError> {
            Ok(self.response.clone())
        }
        async fn send_photo(
            &self,
            _: &str,
            _: &str,
            _: Option<&str>,
        ) -> Result<serde_json::Value, TelegramError> {
            Ok(self.response.clone())
        }
        async fn edit_message_text(
            &self,
            _: &str,
            _: i64,
            _: &str,
            _: Option<&str>,
        ) -> Result<serde_json::Value, TelegramError> {
            Ok(self.response.clone())
        }
        async fn delete_message(
            &self,
            _: &str,
            _: i64,
        ) -> Result<serde_json::Value, TelegramError> {
            Ok(self.response.clone())
        }
        async fn send_inline_keyboard(
            &self,
            _: &str,
            _: &str,
            _: serde_json::Value,
        ) -> Result<serde_json::Value, TelegramError> {
            Ok(self.response.clone())
        }
        async fn get_updates(
            &self,
            _: Option<i64>,
            _: u64,
            _: &[String],
        ) -> Result<serde_json::Value, TelegramError> {
            Ok(self.response.clone())
        }
        async fn set_webhook(
            &self,
            _: &str,
            _: Option<&str>,
            _: &[String],
        ) -> Result<serde_json::Value, TelegramError> {
            Ok(self.response.clone())
        }
        async fn delete_webhook(&self) -> Result<serde_json::Value, TelegramError> {
            Ok(self.response.clone())
        }
        async fn get_me(&self) -> Result<serde_json::Value, TelegramError> {
            Ok(self.response.clone())
        }
        async fn answer_callback_query(
            &self,
            _callback_query_id: &str,
            _text: Option<&str>,
            _show_alert: bool,
        ) -> Result<serde_json::Value, TelegramError> {
            Ok(self.response.clone())
        }
    }
}
