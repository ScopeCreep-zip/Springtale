//! Telegram chat ingestion — the receive loop the daemon used to own.
//!
//! Before [`springtale_connector::chat::ChatSource`] existed, this loop
//! lived in `apps/springtaled/src/runtime/connectors/telegram.rs` and was
//! built from a typed TOML config, so a Telegram connector installed at
//! runtime could never receive chat. The protocol belongs to the crate
//! that speaks it; the runtime only starts and stops it.

use std::sync::Arc;

use async_trait::async_trait;
use secrecy::SecretBox;
use tokio::sync::{mpsc, watch};

use springtale_connector::chat::{ChatMessage, ChatSource};
use springtale_connector::error::ConnectorError;

use crate::client::{TelegramApi, TelegramClient};
use crate::config::TelegramConfig;
use crate::error::TelegramError;

/// Registry name this source reports on every [`ChatMessage`].
pub const CONNECTOR_NAME: &str = "connector-telegram";

/// Telegram's inbound/outbound half: long-polling (or webhook
/// registration) in, `sendMessage` out.
pub struct TelegramChatSource {
    /// Shared with the connector so polling, callback acks and replies
    /// all reuse one Bot API client (and one copy of the bot token).
    /// `Arc` because `polling_loop` takes ownership of a handle and the
    /// callback-ack path clones another into the dispatcher closure.
    client: Arc<TelegramClient>,
    /// `"polling"` or `"webhook"` — anything else is a config error.
    update_mode: String,
    /// Webhook callback URL (required in webhook mode).
    webhook_url: Option<String>,
    /// Webhook secret token (required in webhook mode). Stays wrapped;
    /// only cloned out at the `setWebhook` call site.
    webhook_secret: Option<SecretBox<String>>,
    /// Long-polling timeout in seconds.
    poll_timeout: u64,
}

impl TelegramChatSource {
    /// Build the source from the connector's config and its client.
    pub fn new(
        config: &TelegramConfig,
        client: Arc<TelegramClient>,
    ) -> Result<Self, TelegramError> {
        Ok(Self {
            client,
            update_mode: config.update_mode.clone(),
            webhook_url: config.webhook_url.clone(),
            webhook_secret: config
                .webhook_secret
                .as_ref()
                .map(springtale_crypto::secret_use::clone_into_box),
            poll_timeout: config.poll_timeout,
        })
    }

    /// Register the webhook with Telegram, then park until shutdown.
    ///
    /// The webhook URL must be HTTPS-reachable from Telegram's servers.
    /// The secret token is included in the `X-Telegram-Bot-Api-Secret-Token`
    /// header on every webhook request; the connector's `verify_webhook`
    /// checks it. Inbound updates arrive through the management API's
    /// `/webhook/connector-telegram/...` endpoint, not through this task,
    /// so there is no loop to run — only a registration and a wait.
    async fn run_webhook(&self, mut shutdown: watch::Receiver<bool>) -> Result<(), ConnectorError> {
        let webhook_url = self.webhook_url.as_ref().ok_or_else(|| {
            ConnectorError::ExecutionFailed(
                "webhook mode requires [telegram] webhook_url in config".to_owned(),
            )
        })?;

        let secret = self.webhook_secret.as_ref().ok_or_else(|| {
            ConnectorError::ExecutionFailed(
                "webhook mode requires [telegram] webhook_secret in config — \
                 Telegram needs it to authenticate webhook requests"
                    .to_owned(),
            )
        })?;

        // SECURITY: expose needed for the setWebhook secret_token parameter.
        // header_value clones the secret out of the SecretBox so the
        // resulting String lives only for this stack frame; the original
        // wrapped secret stays zeroize-on-drop. `with_str` can't be used
        // here because set_webhook is async and the returned Future would
        // outlive the closure's borrow of the secret bytes.
        let sec = springtale_crypto::secret_use::header_value(secret);
        self.client
            .set_webhook(webhook_url, Some(&sec), &[])
            .await
            .map_err(|e| {
                ConnectorError::ExecutionFailed(format!("failed to register Telegram webhook: {e}"))
            })?;

        tracing::info!(
            webhook_url = %webhook_url,
            "Telegram webhook registered — incoming messages routed via /webhook/connector-telegram/..."
        );

        // `run` must not return while the connector is live: the runtime
        // treats a return as "this source stopped". Park until shutdown.
        loop {
            if *shutdown.borrow_and_update() {
                break;
            }
            if shutdown.changed().await.is_err() {
                // Sender dropped — the runtime is gone; stop cleanly.
                break;
            }
        }
        Ok(())
    }

    /// Drive the long-polling loop, pushing every update onto `tx`.
    async fn run_polling(
        &self,
        tx: mpsc::Sender<ChatMessage>,
        shutdown: watch::Receiver<bool>,
    ) -> Result<(), ConnectorError> {
        let dispatcher = self.build_dispatcher(tx);

        tracing::info!("Telegram polling started");
        crate::polling::polling_loop(
            self.client.clone(),
            self.poll_timeout,
            vec![],
            dispatcher,
            shutdown,
        )
        .await;
        Ok(())
    }

    /// Build the polling dispatcher: raw update → [`ChatMessage`].
    ///
    /// Rule-engine classification rides along on the message rather than
    /// going out as a separate `ConnectorEvent`: a plain `message` update
    /// is `["message"]`, a `/command` is `["message", "command_received"]`,
    /// and an inline-button press is `["callback_query_received"]`. The
    /// raw update is the payload — the trigger event loop flattens it to
    /// the connector's declared schema before rule matching.
    fn build_dispatcher(
        &self,
        tx: mpsc::Sender<ChatMessage>,
    ) -> Arc<dyn Fn(serde_json::Value) + Send + Sync> {
        // Clone the client into the dispatcher so callback_query updates
        // can be `answerCallbackQuery`'d within Telegram's 10s window —
        // otherwise the user's button keeps spinning.
        let ack_client = self.client.clone();

        Arc::new(move |update: serde_json::Value| {
            if let Some(message) = update.get("message") {
                let tx = tx.clone();
                let msg = message.clone();
                let raw = update.clone();
                tokio::spawn(async move {
                    let user_id = msg
                        .get("from")
                        .and_then(|f| f.get("id"))
                        .and_then(|i| i.as_i64())
                        .map(|i| i.to_string())
                        .unwrap_or_default();
                    let channel_id = msg
                        .get("chat")
                        .and_then(|c| c.get("id"))
                        .and_then(|i| i.as_i64())
                        .map(|i| i.to_string())
                        .unwrap_or_default();
                    let text = msg
                        .get("text")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_owned();

                    let mut events: Vec<&'static str> = vec!["message"];
                    if text.starts_with('/') {
                        events.push("command_received");
                    }

                    let chat_msg =
                        ChatMessage::chat(CONNECTOR_NAME, channel_id, user_id, text, raw)
                            .with_events(events);
                    if let Err(e) = tx.send(chat_msg).await {
                        tracing::error!(error = %e, "failed to send Telegram message to bot");
                    }
                });
            } else if let Some(callback) = update.get("callback_query") {
                // Inline keyboard button press. Route with the
                // callback_data as the message text — handlers can treat
                // it as a command-like input. `raw` preserves the full
                // callback_query object so handlers that need `id` (to
                // answer the query) can reach it.
                let tx = tx.clone();
                let cb = callback.clone();
                let raw = update.clone();
                let ack_client = ack_client.clone();
                tokio::spawn(async move {
                    // Immediately acknowledge so the user's loading
                    // spinner stops. Telegram gives us 10 seconds before
                    // it times out the callback_query; running this
                    // before dispatching keeps the UX snappy.
                    if let Some(callback_id) = cb.get("id").and_then(|v| v.as_str())
                        && let Err(e) = ack_client
                            .answer_callback_query(callback_id, None, false)
                            .await
                    {
                        tracing::warn!(
                            error = %e,
                            "failed to answerCallbackQuery — button spinner will time out"
                        );
                    }
                    let user_id = cb
                        .get("from")
                        .and_then(|f| f.get("id"))
                        .and_then(|i| i.as_i64())
                        .map(|i| i.to_string())
                        .unwrap_or_default();
                    // callback_query.message.chat.id — the chat where the
                    // inline keyboard was sent.
                    let channel_id = cb
                        .get("message")
                        .and_then(|m| m.get("chat"))
                        .and_then(|c| c.get("id"))
                        .and_then(|i| i.as_i64())
                        .map(|i| i.to_string())
                        .unwrap_or_default();
                    let text = cb
                        .get("data")
                        .and_then(|d| d.as_str())
                        .unwrap_or("")
                        .to_owned();

                    let chat_msg =
                        ChatMessage::chat(CONNECTOR_NAME, channel_id, user_id, text, raw)
                            .with_events(["callback_query_received"]);
                    if let Err(e) = tx.send(chat_msg).await {
                        tracing::error!(
                            error = %e,
                            "failed to send Telegram callback_query to bot"
                        );
                    }
                });
            }
        })
    }
}

#[async_trait]
impl ChatSource for TelegramChatSource {
    async fn run(
        &self,
        tx: mpsc::Sender<ChatMessage>,
        shutdown: watch::Receiver<bool>,
    ) -> Result<(), ConnectorError> {
        match self.update_mode.as_str() {
            "webhook" => self.run_webhook(shutdown).await,
            "polling" => self.run_polling(tx, shutdown).await,
            other => Err(ConnectorError::ExecutionFailed(format!(
                "unknown Telegram update_mode '{other}' — expected 'polling' or 'webhook'"
            ))),
        }
    }

    async fn send(&self, channel_id: &str, text: &str) -> Result<(), ConnectorError> {
        self.client
            .send_message(channel_id, text, None, None)
            .await
            .map(|_| ())
            .map_err(ConnectorError::from)
    }
}
