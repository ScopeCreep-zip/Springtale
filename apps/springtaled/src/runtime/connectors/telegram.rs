use std::sync::Arc;

use anyhow::Context;
use tokio::sync::{RwLock, mpsc, watch};

use connector_telegram::client::TelegramApi;
use springtale_connector::registry::store::ConnectorRegistry;
use springtale_core::rule::engine::TriggerEvent;

/// Start the Telegram gateway in polling OR webhook mode.
///
/// The connector is already registered in the registry by the factory system.
///
/// - Polling mode (default): spawns a long-polling loop and bridges incoming
///   messages to `bot_msg_tx`.
/// - Webhook mode: calls Telegram's `setWebhook` to register the callback URL
///   with a secret token. Incoming webhooks are routed via the management
///   API's `/webhook/connector-telegram/...` endpoint, which forwards to
///   `bot_msg_tx` through `AppState`.
///
/// Returns an optional shutdown sender. Polling mode returns `Some` so the
/// daemon can signal graceful stop; webhook mode returns `None`.
pub async fn wire_telegram(
    config: &connector_telegram::TelegramConfig,
    registry: &Arc<RwLock<ConnectorRegistry>>,
    bot_msg_tx: mpsc::Sender<springtale_bot::IncomingMessage>,
    trigger_tx: mpsc::Sender<TriggerEvent>,
) -> anyhow::Result<Option<watch::Sender<bool>>> {
    // Verify connector was registered by factory
    {
        let reg = registry.read().await;
        if reg.get("connector-telegram").is_none() {
            anyhow::bail!("connector-telegram not found in registry — check config");
        }
    }

    match config.update_mode.as_str() {
        "webhook" => start_webhook_mode(config).await.map(|_| None),
        "polling" => start_polling_mode(config, bot_msg_tx, trigger_tx)
            .await
            .map(Some),
        other => anyhow::bail!(
            "unknown Telegram update_mode '{other}' — expected 'polling' or 'webhook'"
        ),
    }
}

/// Emit ConnectorEvent(s) for a raw Telegram update to the rule engine.
///
/// A plain message → `message`; a `/command` → additionally
/// `command_received`; an inline-button press → `callback_query_received`.
/// The raw `update` is the payload — the trigger event loop normalizes it
/// to the connector's declared flat schema before rule matching. Spawns
/// detached sends so the polling dispatcher never blocks.
fn emit_telegram_events(trigger_tx: &mpsc::Sender<TriggerEvent>, update: &serde_json::Value) {
    let mut events: Vec<&'static str> = Vec::new();
    if let Some(message) = update.get("message") {
        events.push("message");
        if message
            .get("text")
            .and_then(|t| t.as_str())
            .is_some_and(|t| t.starts_with('/'))
        {
            events.push("command_received");
        }
    } else if update.get("callback_query").is_some() {
        events.push("callback_query_received");
    }

    for event in events {
        let tx = trigger_tx.clone();
        let payload = update.clone();
        tokio::spawn(async move {
            let evt = TriggerEvent {
                trigger_type: "ConnectorEvent".to_owned(),
                connector: Some("connector-telegram".to_owned()),
                event: Some(event.to_owned()),
                payload,
            };
            if let Err(e) = tx.send(evt).await {
                tracing::warn!(error = %e, "failed to emit Telegram ConnectorEvent");
            }
        });
    }
}

/// Register a webhook with Telegram's setWebhook API.
///
/// The webhook URL must be HTTPS-reachable from Telegram's servers.
/// The secret token will be included in the `X-Telegram-Bot-Api-Secret-Token`
/// header on every webhook request for verification (see `verify_webhook`).
async fn start_webhook_mode(config: &connector_telegram::TelegramConfig) -> anyhow::Result<()> {
    let webhook_url = config
        .webhook_url
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("webhook mode requires [telegram] webhook_url in config"))?;

    let secret = config.webhook_secret.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "webhook mode requires [telegram] webhook_secret in config — \
             Telegram needs it to authenticate webhook requests"
        )
    })?;

    let token = springtale_crypto::secret_use::clone_into_box(&config.bot_token);
    let client = connector_telegram::TelegramClient::new(&config.api_base, token)
        .context("failed to create Telegram client for webhook setup")?;

    // header_value clones the secret out of the SecretBox so the
    // resulting String lives only for this stack frame; the original
    // wrapped secret stays zeroize-on-drop. `with_str` can't be used
    // here because set_webhook is async and the returned Future would
    // outlive the closure's borrow of the secret bytes.
    let sec = springtale_crypto::secret_use::header_value(secret);
    client
        .set_webhook(webhook_url, Some(&sec), &[])
        .await
        .context("failed to register Telegram webhook")?;

    tracing::info!(
        webhook_url = %webhook_url,
        "Telegram webhook registered — incoming messages routed via /webhook/connector-telegram/..."
    );
    Ok(())
}

/// Start the long-polling loop and bridge incoming messages to the bot.
async fn start_polling_mode(
    config: &connector_telegram::TelegramConfig,
    bot_msg_tx: mpsc::Sender<springtale_bot::IncomingMessage>,
    trigger_tx: mpsc::Sender<TriggerEvent>,
) -> anyhow::Result<watch::Sender<bool>> {
    let poll_token = springtale_crypto::secret_use::clone_into_box(&config.bot_token);
    let poll_client = connector_telegram::TelegramClient::new(&config.api_base, poll_token)
        .context("failed to create Telegram polling client")?;

    let poll_client = Arc::new(poll_client);
    let poll_timeout = config.poll_timeout;

    // Clone the poll_client into the dispatcher so callback_query
    // updates can be `answerCallbackQuery`'d within Telegram's 10s
    // window — otherwise the user's button keeps spinning. We do this
    // directly via the client rather than going through the registry
    // because the dispatcher runs inside the polling task and already
    // holds a reference to the same client.
    let ack_client = poll_client.clone();
    let evt_tx = trigger_tx.clone();
    let poll_dispatcher: Arc<dyn Fn(serde_json::Value) + Send + Sync> = Arc::new(
        move |update: serde_json::Value| {
            let ack_client = ack_client.clone();

            // Rule path: emit ConnectorEvent(s) to the engine so
            // ConnectorEvent recipes (telegram-echo, telegram-cmd-broadcast,
            // …) actually fire on polling-delivered messages — not just the
            // bot chat path below. The raw `update` is flattened to the
            // declared schema centrally, in the trigger event loop.
            emit_telegram_events(&evt_tx, &update);

            if let Some(message) = update.get("message") {
                let tx = bot_msg_tx.clone();
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

                    let incoming = springtale_bot::IncomingMessage {
                        user_id,
                        channel_id,
                        text,
                        source_connector: "connector-telegram".to_owned(),
                        raw,
                    };
                    if let Err(e) = tx.send(incoming).await {
                        tracing::error!(error = %e, "failed to send Telegram message to bot");
                    }
                });
            } else if let Some(callback) = update.get("callback_query") {
                // Inline keyboard button press. Route to the bot with the
                // callback_data as the message text — handlers can treat it
                // as a command-like input. The `raw` field preserves the
                // full callback_query object so handlers that need `id`
                // (to answer the query) can access it.
                let tx = bot_msg_tx.clone();
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

                    let incoming = springtale_bot::IncomingMessage {
                        user_id,
                        channel_id,
                        text,
                        source_connector: "connector-telegram".to_owned(),
                        raw,
                    };
                    if let Err(e) = tx.send(incoming).await {
                        tracing::error!(error = %e, "failed to send Telegram callback_query to bot");
                    }
                });
            }
        },
    );

    let (shutdown_tx, poll_shutdown_rx) = watch::channel(false);

    tokio::spawn(async move {
        connector_telegram::polling::polling_loop(
            poll_client,
            poll_timeout,
            vec![],
            poll_dispatcher,
            poll_shutdown_rx,
        )
        .await;
    });

    tracing::info!("Telegram polling started");
    Ok(shutdown_tx)
}
