use std::sync::Arc;

use async_trait::async_trait;
use secrecy::SecretBox;
use tokio::sync::Mutex;

use springtale_connector::connector::trait_::{ActionResult, Connector, EventHandler};
use springtale_connector::error::ConnectorError;
use springtale_connector::manifest::types::{
    ActionDecl, Capability, ConnectorManifest, DataDisclosure, TriggerDecl,
};
use springtale_connector::{Subscription, SubscriptionCounter, SubscriptionId};

use crate::actions;
use crate::chat::TelegramChatSource;
use crate::client::TelegramClient;
use crate::config::TelegramConfig;
use crate::triggers;
use springtale_connector::manifest::SignatureAlgorithm;

/// Telegram connector.
/// Provides Telegram Bot API integration with polling or webhook triggers.
pub struct TelegramConnector {
    client: Arc<TelegramClient>,
    manifest: ConnectorManifest,
    triggers: Vec<TriggerDecl>,
    actions: Vec<ActionDecl>,
    handlers: Arc<Mutex<Vec<(SubscriptionId, String, EventHandler)>>>,
    sub_counter: SubscriptionCounter,
    /// Optional webhook secret token (clone of config.webhook_secret).
    /// Used to verify incoming webhook requests by the daemon.
    webhook_secret: Option<SecretBox<String>>,
    /// Receive loop + outbound half, handed to the runtime via
    /// [`Connector::chat_source`].
    chat: Arc<TelegramChatSource>,
}

impl TelegramConnector {
    pub fn new(config: &TelegramConfig) -> Result<Self, crate::error::TelegramError> {
        let trigger_decls = triggers::trigger_declarations();
        let action_decls = actions::action_declarations();
        let manifest = build_manifest(&trigger_decls, &action_decls);

        let token = springtale_crypto::secret_use::clone_into_box(&config.bot_token);
        let client = Arc::new(TelegramClient::new(&config.api_base, token)?);

        let webhook_secret = config
            .webhook_secret
            .as_ref()
            .map(springtale_crypto::secret_use::clone_into_box);

        let chat = Arc::new(TelegramChatSource::new(config, Arc::clone(&client))?);

        Ok(Self {
            client,
            manifest,
            triggers: trigger_decls,
            actions: action_decls,
            handlers: Arc::new(Mutex::new(Vec::new())),
            sub_counter: SubscriptionCounter::new(),
            webhook_secret,
            chat,
        })
    }

    /// Dispatch a parsed update to registered handlers.
    pub async fn dispatch_update(&self, update: &serde_json::Value) {
        if let Some(message) = update.get("message") {
            let text = message.get("text").and_then(|t| t.as_str()).unwrap_or("");

            if text.starts_with('/') {
                let (command, args) = crate::webhook::parse_command(text);
                let mut payload = message.clone();
                if let Some(obj) = payload.as_object_mut() {
                    obj.insert("command".to_owned(), serde_json::Value::String(command));
                    obj.insert("args".to_owned(), serde_json::Value::String(args));
                }
                self.dispatch_to_handlers("command_received", payload).await;
            }

            // Always fire message_received (even for commands)
            self.dispatch_to_handlers("message_received", message.clone())
                .await;
        } else if let Some(callback_query) = update.get("callback_query") {
            // Inline keyboard button press — fire callback_query_received.
            // The payload contains the full callback_query object with id, from,
            // message, and data fields (see triggers/callback_query_received schema).
            self.dispatch_to_handlers("callback_query_received", callback_query.clone())
                .await;
        }
    }

    async fn dispatch_to_handlers(&self, trigger_name: &str, payload: serde_json::Value) {
        let handlers = self.handlers.lock().await;
        for (_id, registered, handler) in handlers.iter() {
            if registered == trigger_name {
                handler(payload.clone());
            }
        }
    }
}

#[async_trait]
impl Connector for TelegramConnector {
    fn triggers(&self) -> &[TriggerDecl] {
        &self.triggers
    }

    fn actions(&self) -> &[ActionDecl] {
        &self.actions
    }

    async fn execute(
        &self,
        action: &str,
        input: serde_json::Value,
    ) -> Result<ActionResult, ConnectorError> {
        match action {
            "send_message" => actions::send_message::execute(self.client.as_ref(), &input)
                .await
                .map_err(ConnectorError::from),
            "send_photo" => actions::send_photo::execute(self.client.as_ref(), &input)
                .await
                .map_err(ConnectorError::from),
            "edit_message" => actions::edit_message::execute(self.client.as_ref(), &input)
                .await
                .map_err(ConnectorError::from),
            "delete_message" => actions::delete_message::execute(self.client.as_ref(), &input)
                .await
                .map_err(ConnectorError::from),
            "send_inline_keyboard" => {
                actions::send_inline_keyboard::execute(self.client.as_ref(), &input)
                    .await
                    .map_err(ConnectorError::from)
            }
            "answer_callback_query" => {
                actions::answer_callback_query::execute(self.client.as_ref(), &input)
                    .await
                    .map_err(ConnectorError::from)
            }
            "onboard_url" => actions::onboard_url::execute(self.client.as_ref(), &input)
                .await
                .map_err(ConnectorError::from),
            "discover_destinations" => {
                actions::discover_destinations::execute(self.client.as_ref(), &input)
                    .await
                    .map_err(ConnectorError::from)
            }
            unknown => Err(ConnectorError::ExecutionFailed(format!(
                "unknown action: {unknown}"
            ))),
        }
    }

    async fn on_event(
        &self,
        trigger: &str,
        handler: EventHandler,
    ) -> Result<Subscription, ConnectorError> {
        let valid_triggers = [
            "message_received",
            "command_received",
            "callback_query_received",
        ];
        if !valid_triggers.contains(&trigger) {
            return Err(ConnectorError::ExecutionFailed(format!(
                "unknown trigger: {trigger}"
            )));
        }

        let id = self.sub_counter.next();
        {
            let mut handlers = self.handlers.lock().await;
            handlers.push((id, trigger.to_owned(), handler));
        }

        tracing::info!(trigger = trigger, "registered Telegram event handler");
        Ok(Subscription {
            id,
            trigger: trigger.to_owned(),
        })
    }

    async fn remove_event(&self, sub: &Subscription) -> Result<(), ConnectorError> {
        let mut handlers = self.handlers.lock().await;
        handlers.retain(|(id, _, _)| *id != sub.id);
        tracing::info!(id = ?sub.id, trigger = %sub.trigger, "removed Telegram event handler");
        Ok(())
    }

    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    fn chat_source(&self) -> Option<springtale_connector::chat::SharedChatSource> {
        Some(self.chat.clone())
    }

    fn mention_extractor(&self) -> Option<&dyn springtale_connector::mention::MentionExtractor> {
        Some(&crate::mention::TELEGRAM_MENTION_EXTRACTOR)
    }

    fn normalize_event(&self, trigger: &str, raw: serde_json::Value) -> serde_json::Value {
        crate::triggers::normalize::normalize(trigger, &raw)
    }

    /// Verify an incoming webhook request using the `X-Telegram-Bot-Api-Secret-Token` header.
    ///
    /// Telegram sets this header on every webhook POST when `set_webhook` was
    /// called with a `secret_token`. Constant-time string comparison prevents
    /// timing attacks.
    async fn verify_webhook(
        &self,
        headers: &std::collections::HashMap<String, String>,
        _body: &[u8],
    ) -> Result<(), ConnectorError> {
        let expected = self.webhook_secret.as_ref().ok_or_else(|| {
            ConnectorError::ExecutionFailed(
                "webhook_secret not configured for connector-telegram".to_owned(),
            )
        })?;

        // Header name is case-insensitive but the standard form is lowercase in HTTP/2+.
        // Check both common casings to handle any reverse proxy normalization.
        let received = headers
            .get("x-telegram-bot-api-secret-token")
            .or_else(|| headers.get("X-Telegram-Bot-Api-Secret-Token"))
            .ok_or_else(|| {
                ConnectorError::ExecutionFailed(
                    "missing X-Telegram-Bot-Api-Secret-Token header".to_owned(),
                )
            })?;

        crate::webhook::verify_webhook_secret(expected, received)
            .map_err(|e| ConnectorError::ExecutionFailed(e.to_string()))
    }
}

/// Build the connector's manifest. The factory calls this with no config-derived
/// parts so the manifest is available without instantiating the connector.
pub(crate) fn build_manifest(
    triggers: &[TriggerDecl],
    actions: &[ActionDecl],
) -> ConnectorManifest {
    ConnectorManifest {
        name: "connector-telegram".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        author: "Springtale".to_owned(),
        description: "Telegram Bot connector — messaging, commands, inline keyboards.".to_owned(),
        capabilities: vec![Capability::NetworkOutbound {
            host: "api.telegram.org".to_owned(),
        }],
        triggers: triggers.to_vec(),
        actions: actions.to_vec(),
        data_disclosure: vec![DataDisclosure {
            data_type: "chat messages".to_owned(),
            purpose: "sending and receiving messages via Telegram Bot API".to_owned(),
            destination: "api.telegram.org".to_owned(),
        }],
        roles: vec![],
        wasm_hash: None,
        signature_alg: SignatureAlgorithm::default(),
        signature: None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn test_config() -> TelegramConfig {
        TelegramConfig {
            bot_token: SecretBox::new(Box::new("123456:ABC-DEF".to_owned())),
            api_base: "https://api.telegram.org".to_owned(),
            update_mode: "polling".to_owned(),
            webhook_url: None,
            webhook_secret: None,
            poll_timeout: 30,
        }
    }

    #[test]
    fn test_manifest_name() {
        let connector = TelegramConnector::new(&test_config()).unwrap();
        assert_eq!(connector.manifest().name, "connector-telegram");
    }

    #[test]
    fn test_manifest_capabilities() {
        let connector = TelegramConnector::new(&test_config()).unwrap();
        let caps = &connector.manifest().capabilities;
        assert_eq!(caps.len(), 1);
        assert!(
            matches!(&caps[0], Capability::NetworkOutbound { host } if host == "api.telegram.org")
        );
    }

    #[test]
    fn test_trigger_count() {
        let connector = TelegramConnector::new(&test_config()).unwrap();
        assert_eq!(connector.triggers().len(), 3);
    }

    #[test]
    fn test_action_count() {
        let connector = TelegramConnector::new(&test_config()).unwrap();
        // 6 messaging actions + D1's `onboard_url` deep-link + D1's
        // `discover_destinations` getUpdates wrapper.
        assert_eq!(connector.actions().len(), 8);
    }

    #[tokio::test]
    async fn test_execute_unknown_action() {
        let connector = TelegramConnector::new(&test_config()).unwrap();
        let result = connector.execute("ban_user", serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_on_event_valid() {
        let connector = TelegramConnector::new(&test_config()).unwrap();
        let handler: EventHandler = Box::new(|_| {});
        assert!(
            connector
                .on_event("message_received", handler)
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn test_on_event_invalid() {
        let connector = TelegramConnector::new(&test_config()).unwrap();
        let handler: EventHandler = Box::new(|_| {});
        assert!(connector.on_event("nonexistent", handler).await.is_err());
    }
}
