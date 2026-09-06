use std::time::Duration;

use async_trait::async_trait;
use secrecy::SecretBox;
use serde::Serialize;
use teloxide_core::Bot;
use teloxide_core::payloads::setters::*;
use teloxide_core::requests::Requester;
use teloxide_core::types::{
    AllowedUpdate, CallbackQueryId, ChatId, FileId, InlineKeyboardMarkup, InputFile, MessageId,
    ParseMode, Recipient, ReplyParameters,
};
use url::Url;

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

/// Telegram Bot API client backed by [`teloxide_core::Bot`].
///
/// The SDK owns URL building, JSON/multipart encoding, `{ok, result}`
/// unwrapping and `retry_after` parsing. This type only adapts the
/// connector's string-and-JSON [`TelegramApi`] surface onto the SDK's
/// typed requests so the action modules and their mock tests stay
/// untouched.
pub struct TelegramClient {
    bot: Bot,
}

/// Per-request wall-clock timeout. Must exceed the long-poll timeout
/// (`poll_timeout`, default 30s) or every `getUpdates` call would abort
/// before Telegram answers. Same value the hand-rolled client used.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

impl TelegramClient {
    pub fn new(api_base: &str, bot_token: SecretBox<String>) -> Result<Self, TelegramError> {
        let api_url = Url::parse(api_base)
            .map_err(|e| TelegramError::InvalidConfig(format!("invalid api_base: {e}")))?;

        // teloxide-core 0.13 pins reqwest 0.12 while the workspace's
        // `safe_http` builder is reqwest 0.13, so the two `Client` types are
        // distinct and the shared factory cannot be used here. Mirror its
        // policy on the SDK's builder instead: rustls only (the `rustls`
        // feature is the sole TLS backend enabled), bounded connect and
        // request timeouts, no proxy picked up from `TELOXIDE_PROXY`.
        let http = teloxide_core::net::default_reqwest_settings()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| TelegramError::RequestFailed(format!("failed to build client: {e}")))?;

        // SECURITY: expose needed once to hand the token to the SDK, which
        // places it in the request path per Telegram's Bot API design.
        let bot =
            springtale_crypto::secret_use::with_str(&bot_token, |tok| Bot::with_client(tok, http))
                .set_api_url(api_url);

        Ok(Self { bot })
    }
}

/// Telegram accepts a numeric chat id or an `@channelusername`.
fn recipient(chat_id: &str) -> Result<Recipient, TelegramError> {
    if let Ok(id) = chat_id.parse::<i64>() {
        return Ok(Recipient::Id(ChatId(id)));
    }
    if chat_id.starts_with('@') {
        return Ok(Recipient::ChannelUsername(chat_id.to_owned()));
    }
    Err(TelegramError::InvalidInput(
        "chat_id must be a numeric id or an @username".to_owned(),
    ))
}

fn message_id(id: i64) -> Result<MessageId, TelegramError> {
    i32::try_from(id)
        .map(MessageId)
        .map_err(|_| TelegramError::InvalidInput(format!("message_id out of range: {id}")))
}

/// "Markdown" | "MarkdownV2" | "HTML" — the Bot API's own spellings.
fn parse_mode(mode: &str) -> Result<ParseMode, TelegramError> {
    serde_json::from_value(serde_json::Value::String(mode.to_owned()))
        .map_err(|_| TelegramError::InvalidInput(format!("unknown parse_mode: {mode}")))
}

/// Update-type names as the Bot API spells them (`message`, `callback_query`, ...).
fn allowed_updates(names: &[String]) -> Result<Vec<AllowedUpdate>, TelegramError> {
    names
        .iter()
        .map(|n| {
            serde_json::from_value(serde_json::Value::String(n.clone()))
                .map_err(|_| TelegramError::InvalidInput(format!("unknown update type: {n}")))
        })
        .collect()
}

/// A photo is either an HTTP(S) URL Telegram fetches itself or a
/// previously uploaded `file_id`.
fn photo_input(photo: &str) -> Result<InputFile, TelegramError> {
    if photo.starts_with("https://") || photo.starts_with("http://") {
        let url = Url::parse(photo)
            .map_err(|e| TelegramError::InvalidInput(format!("invalid photo URL: {e}")))?;
        return Ok(InputFile::url(url));
    }
    Ok(InputFile::file_id(FileId(photo.to_owned())))
}

/// Re-encode an SDK model as the JSON the action modules already consume.
fn encode<T: Serialize>(value: &T) -> Result<serde_json::Value, TelegramError> {
    serde_json::to_value(value)
        .map_err(|e| TelegramError::RequestFailed(format!("failed to encode response: {e}")))
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
        let mut req = self.bot.send_message(recipient(chat_id)?, text);
        if let Some(pm) = parse_mode {
            req = req.parse_mode(self::parse_mode(pm)?);
        }
        if let Some(reply_id) = reply_to_message_id {
            req = req.reply_parameters(ReplyParameters::new(message_id(reply_id)?));
        }
        encode(&req.await?)
    }

    async fn send_photo(
        &self,
        chat_id: &str,
        photo: &str,
        caption: Option<&str>,
    ) -> Result<serde_json::Value, TelegramError> {
        let mut req = self
            .bot
            .send_photo(recipient(chat_id)?, photo_input(photo)?);
        if let Some(cap) = caption {
            req = req.caption(cap);
        }
        encode(&req.await?)
    }

    async fn edit_message_text(
        &self,
        chat_id: &str,
        message_id: i64,
        text: &str,
        parse_mode: Option<&str>,
    ) -> Result<serde_json::Value, TelegramError> {
        let mut req =
            self.bot
                .edit_message_text(recipient(chat_id)?, self::message_id(message_id)?, text);
        if let Some(pm) = parse_mode {
            req = req.parse_mode(self::parse_mode(pm)?);
        }
        encode(&req.await?)
    }

    async fn delete_message(
        &self,
        chat_id: &str,
        message_id: i64,
    ) -> Result<serde_json::Value, TelegramError> {
        let req = self
            .bot
            .delete_message(recipient(chat_id)?, self::message_id(message_id)?);
        encode(&req.await?)
    }

    async fn send_inline_keyboard(
        &self,
        chat_id: &str,
        text: &str,
        inline_keyboard: serde_json::Value,
    ) -> Result<serde_json::Value, TelegramError> {
        let markup: InlineKeyboardMarkup =
            serde_json::from_value(serde_json::json!({ "inline_keyboard": inline_keyboard }))
                .map_err(|e| {
                    TelegramError::InvalidInput(format!("invalid inline_keyboard: {e}"))
                })?;
        let req = self
            .bot
            .send_message(recipient(chat_id)?, text)
            .reply_markup(markup);
        encode(&req.await?)
    }

    async fn get_updates(
        &self,
        offset: Option<i64>,
        timeout: u64,
        allowed_updates: &[String],
    ) -> Result<serde_json::Value, TelegramError> {
        let timeout = u32::try_from(timeout).map_err(|_| {
            TelegramError::InvalidConfig(format!("poll_timeout too large: {timeout}"))
        })?;
        let mut req = self.bot.get_updates().timeout(timeout);
        if let Some(off) = offset {
            let off = i32::try_from(off).map_err(|_| {
                TelegramError::PollingFailed(format!("update offset out of range: {off}"))
            })?;
            req = req.offset(off);
        }
        if !allowed_updates.is_empty() {
            req = req.allowed_updates(self::allowed_updates(allowed_updates)?);
        }
        encode(&req.await?)
    }

    async fn set_webhook(
        &self,
        url: &str,
        secret_token: Option<&str>,
        allowed_updates: &[String],
    ) -> Result<serde_json::Value, TelegramError> {
        let url = Url::parse(url)
            .map_err(|e| TelegramError::InvalidConfig(format!("invalid webhook_url: {e}")))?;
        let mut req = self.bot.set_webhook(url);
        if let Some(token) = secret_token {
            req = req.secret_token(token);
        }
        if !allowed_updates.is_empty() {
            req = req.allowed_updates(self::allowed_updates(allowed_updates)?);
        }
        encode(&req.await?)
    }

    async fn delete_webhook(&self) -> Result<serde_json::Value, TelegramError> {
        encode(&self.bot.delete_webhook().await?)
    }

    async fn get_me(&self) -> Result<serde_json::Value, TelegramError> {
        encode(&self.bot.get_me().await?)
    }

    async fn answer_callback_query(
        &self,
        callback_query_id: &str,
        text: Option<&str>,
        show_alert: bool,
    ) -> Result<serde_json::Value, TelegramError> {
        let mut req = self
            .bot
            .answer_callback_query(CallbackQueryId(callback_query_id.to_owned()))
            .show_alert(show_alert);
        if let Some(t) = text {
            req = req.text(t);
        }
        encode(&req.await?)
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
