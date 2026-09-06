use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use springtale_connector::connector::trait_::{ActionResult, Connector, EventHandler};
use springtale_connector::error::ConnectorError;
use springtale_connector::manifest::types::{
    ActionDecl, Capability, ConnectorManifest, DataDisclosure, TriggerDecl,
};
use springtale_connector::{Subscription, SubscriptionCounter, SubscriptionId};

use crate::actions;
use crate::chat::KickChatSource;
use crate::client::{KickApi, KickClient};
use crate::config::KickConfig;
use crate::triggers;
use crate::webhook;
use springtale_connector::manifest::SignatureAlgorithm;

/// Kick connector.
///
/// Provides Kick platform integration with OAuth 2.1 PKCE authentication,
/// REST API actions, and webhook-driven triggers. When a trigger handler
/// is registered via `on_event()`, the connector subscribes with Kick's
/// event subscription API so webhooks are delivered.
pub struct KickConnector {
    client: Arc<KickClient>,
    /// Inbound/outbound chat half. Kick's inbound stream is its own
    /// webhook dispatch — see [`KickChatSource`].
    chat: Arc<KickChatSource>,
    manifest: ConnectorManifest,
    triggers: Vec<TriggerDecl>,
    actions: Vec<ActionDecl>,
    handlers: Arc<Mutex<Vec<(SubscriptionId, String, EventHandler)>>>,
    /// Webhook callback URL where Kick sends events (from config).
    webhook_callback_url: Option<String>,
    /// Kick event types we've already subscribed to (avoid duplicate subscriptions).
    subscribed_events: Mutex<HashSet<String>>,
    sub_counter: SubscriptionCounter,
    /// API base, needed to fetch Kick's webhook public key lazily.
    api_base: String,
    /// Cached PEM public key from `GET /public/v1/public-key`.
    webhook_public_key: Mutex<Option<String>>,
    /// Seen `Kick-Event-Message-Id`s for replay protection (in-memory:
    /// the trait hands us no store; see `webhook::replay`).
    replay_cache: Mutex<webhook::ReplayCache>,
}

/// Map a connector trigger name to the Kick event type(s) to subscribe to.
fn trigger_to_kick_events(trigger: &str) -> &[&str] {
    match trigger {
        "chat_message" => &["chat.message.sent"],
        "stream_live" | "stream_offline" => &["livestream.status.updated"],
        "channel_followed" => &["channel.followed"],
        _ => &[],
    }
}

impl KickConnector {
    /// Create a new Kick connector with an authenticated client.
    ///
    /// The `access_token` should be obtained via the OAuth 2.1 PKCE flow
    /// (see `auth::exchange_code`).
    pub fn new(
        config: &KickConfig,
        access_token: secrecy::SecretBox<String>,
    ) -> Result<Self, crate::error::KickError> {
        let trigger_decls = triggers::trigger_declarations();
        let action_decls = actions::action_declarations();
        let manifest = build_manifest(&trigger_decls, &action_decls);
        let client = Arc::new(KickClient::new(&config.api_base, access_token)?);
        let chat = Arc::new(KickChatSource::new(Arc::clone(&client)));

        Ok(Self {
            client,
            chat,
            manifest,
            triggers: trigger_decls,
            actions: action_decls,
            handlers: Arc::new(Mutex::new(Vec::new())),
            webhook_callback_url: config.webhook_callback_url.clone(),
            subscribed_events: Mutex::new(HashSet::new()),
            sub_counter: SubscriptionCounter::new(),
            api_base: config.api_base.clone(),
            webhook_public_key: Mutex::new(None),
            replay_cache: Mutex::new(webhook::ReplayCache::default()),
        })
    }

    /// Kick's webhook signing public key, fetched once and cached for
    /// the connector's lifetime. The lock is held across the fetch so a
    /// burst of first webhooks issues a single request.
    async fn webhook_public_key(&self) -> Result<String, crate::error::KickError> {
        let mut cached = self.webhook_public_key.lock().await;
        if let Some(pem) = cached.as_ref() {
            return Ok(pem.clone());
        }
        let pem = KickClient::fetch_public_key(&self.api_base).await?;
        *cached = Some(pem.clone());
        Ok(pem)
    }

    /// Dispatch a webhook event to registered handlers by trigger name.
    pub async fn dispatch_webhook(&self, trigger_name: &str, payload: serde_json::Value) {
        // Chat webhooks also feed the connector's ChatSource, which is
        // the only path a Kick chat message has to the bot runtime.
        self.chat.ingest(trigger_name, &payload);
        let handlers = self.handlers.lock().await;
        for (_id, registered, handler) in handlers.iter() {
            if registered == trigger_name {
                handler(payload.clone());
            }
        }
    }

    /// Dispatch a raw Kick webhook by event type header + payload.
    ///
    /// This is the method springtaled's management API should call after
    /// verifying the RSA signature. It handles the Kick-specific mapping
    /// from event types to trigger names, including the livestream
    /// `is_live` branching logic.
    pub async fn dispatch_raw_webhook(&self, kick_event_type: &str, payload: serde_json::Value) {
        // For most events, use the direct mapping
        let trigger_name = if kick_event_type == "livestream.status.updated" {
            // Livestream events need payload inspection to determine trigger
            webhook::resolve_livestream_trigger(&payload)
        } else {
            webhook::event_type_to_trigger(kick_event_type)
        };

        if let Some(name) = trigger_name {
            self.dispatch_webhook(name, payload).await;
        } else {
            tracing::debug!(
                event_type = kick_event_type,
                "no trigger mapping for Kick event type"
            );
        }
    }
}

#[async_trait]
impl Connector for KickConnector {
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
        let client: &dyn KickApi = self.client.as_ref();
        match action {
            "send_chat" => actions::send_chat::execute(client, &input)
                .await
                .map_err(ConnectorError::from),
            "get_channel" => actions::get_channel::execute(client, &input)
                .await
                .map_err(ConnectorError::from),
            "get_stream" => actions::get_stream::execute(client, &input)
                .await
                .map_err(ConnectorError::from),
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
            "chat_message",
            "stream_live",
            "stream_offline",
            "channel_followed",
        ];
        if !valid_triggers.contains(&trigger) {
            return Err(ConnectorError::ExecutionFailed(format!(
                "unknown trigger: {trigger}"
            )));
        }

        // Store the handler
        let id = self.sub_counter.next();
        {
            let mut handlers = self.handlers.lock().await;
            handlers.push((id, trigger.to_owned(), handler));
        }

        // Subscribe with Kick's API for the corresponding event types
        // (only if webhook_callback_url is configured and not already subscribed)
        if let Some(ref callback_url) = self.webhook_callback_url {
            let kick_events = trigger_to_kick_events(trigger);
            let mut subscribed = self.subscribed_events.lock().await;

            let new_events: Vec<&str> = kick_events
                .iter()
                .filter(|e| !subscribed.contains(**e))
                .copied()
                .collect();

            if !new_events.is_empty() {
                match self
                    .client
                    .subscribe_events(&new_events, callback_url)
                    .await
                {
                    Ok(_) => {
                        for event in &new_events {
                            subscribed.insert((*event).to_owned());
                        }
                        tracing::info!(
                            trigger = trigger,
                            events = ?new_events,
                            "subscribed to Kick webhook events"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            trigger = trigger,
                            error = %e,
                            "failed to subscribe to Kick events (handler registered locally)"
                        );
                    }
                }
            }
        }

        tracing::info!(trigger = trigger, "registered Kick event handler");
        Ok(Subscription {
            id,
            trigger: trigger.to_owned(),
        })
    }

    /// Verify a Kick webhook: RSA-PKCS1v15-SHA256 over
    /// `{message_id}.{timestamp}.{body}` with Kick's published key, then
    /// replay protection — the timestamp must be within five minutes and
    /// the message id must not have been seen in the last hour. Signature
    /// and body are never logged or echoed in errors.
    async fn verify_webhook(
        &self,
        headers: &std::collections::HashMap<String, String>,
        body: &[u8],
    ) -> Result<(), ConnectorError> {
        let message_id = webhook::required_header(headers, webhook::HEADER_MESSAGE_ID)?;
        let timestamp = webhook::required_header(headers, webhook::HEADER_TIMESTAMP)?;
        let signature = webhook::required_header(headers, webhook::HEADER_SIGNATURE)?;

        let public_key = self.webhook_public_key().await?;
        webhook::verify_webhook(&public_key, message_id, timestamp, body, signature)?;

        // Replay checks run only after the signature is proven genuine so
        // an attacker cannot pre-poison the seen-id cache.
        webhook::check_timestamp(timestamp, chrono::Utc::now())?;
        self.replay_cache
            .lock()
            .await
            .check_and_record(message_id, std::time::Instant::now())?;
        Ok(())
    }

    async fn remove_event(&self, sub: &Subscription) -> Result<(), ConnectorError> {
        let mut handlers = self.handlers.lock().await;
        handlers.retain(|(id, _, _)| *id != sub.id);
        tracing::info!(id = ?sub.id, trigger = %sub.trigger, "removed Kick event handler");
        Ok(())
    }

    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    /// Read a verified Kick webhook into the chat it carries, so
    /// `chat.message.sent` reaches the bot through the management API's
    /// ingress. Kick has no polling gateway — this is the only route.
    async fn ingest_webhook(
        &self,
        trigger: &str,
        headers: &std::collections::HashMap<String, String>,
        payload: &serde_json::Value,
    ) -> springtale_connector::webhook::WebhookIngest {
        webhook::ingest_event(trigger, headers, payload)
    }

    fn chat_source(&self) -> Option<springtale_connector::chat::SharedChatSource> {
        Some(self.chat.clone())
    }
}

/// Build the connector's manifest. The factory calls this with no config-derived
/// parts so the manifest is available without instantiating the connector.
pub(crate) fn build_manifest(
    triggers: &[TriggerDecl],
    actions: &[ActionDecl],
) -> ConnectorManifest {
    ConnectorManifest {
        name: "connector-kick".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        author: "Springtale".to_owned(),
        description: "Kick connector — OAuth 2.1 PKCE auth, chat, channels, livestreams."
            .to_owned(),
        capabilities: vec![
            Capability::NetworkOutbound {
                host: "api.kick.com".to_owned(),
            },
            Capability::NetworkOutbound {
                host: "id.kick.com".to_owned(),
            },
        ],
        triggers: triggers.to_vec(),
        actions: actions.to_vec(),
        data_disclosure: vec![
            DataDisclosure {
                data_type: "chat messages".to_owned(),
                purpose: "sending and receiving chat messages in Kick channels".to_owned(),
                destination: "api.kick.com".to_owned(),
            },
            DataDisclosure {
                data_type: "channel/stream metadata".to_owned(),
                purpose: "monitoring channel status for automation triggers".to_owned(),
                destination: "api.kick.com".to_owned(),
            },
        ],
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

    fn test_connector() -> KickConnector {
        let config = KickConfig {
            client_id: "test_id".to_owned(),
            client_secret: secrecy::SecretBox::new(Box::new("secret".to_owned())),
            redirect_uri: "http://localhost:3000/callback".to_owned(),
            scopes: vec!["user:read".to_owned()],
            api_base: "https://api.kick.com".to_owned(),
            oauth_base: "https://id.kick.com".to_owned(),
            webhook_callback_url: None,
        };
        KickConnector::new(
            &config,
            secrecy::SecretBox::new(Box::new("test_access_token".to_owned())),
        )
        .unwrap()
    }

    #[test]
    fn test_manifest_name() {
        let connector = test_connector();
        assert_eq!(connector.manifest().name, "connector-kick");
    }

    #[test]
    fn test_manifest_capabilities() {
        let connector = test_connector();
        let caps = &connector.manifest().capabilities;
        assert_eq!(caps.len(), 2);
        let hosts: Vec<&str> = caps
            .iter()
            .filter_map(|c| match c {
                Capability::NetworkOutbound { host } => Some(host.as_str()),
                _ => None,
            })
            .collect();
        assert!(hosts.contains(&"api.kick.com"));
        assert!(hosts.contains(&"id.kick.com"));
    }

    #[test]
    fn test_four_triggers() {
        let connector = test_connector();
        assert_eq!(connector.triggers().len(), 4);
    }

    #[test]
    fn test_three_actions() {
        let connector = test_connector();
        assert_eq!(connector.actions().len(), 3);
    }

    #[tokio::test]
    async fn test_execute_unknown_action() {
        let connector = test_connector();
        let result = connector.execute("ban_user", serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_on_event_valid() {
        let connector = test_connector();
        let handler: EventHandler = Box::new(|_| {});
        assert!(connector.on_event("chat_message", handler).await.is_ok());
    }

    #[tokio::test]
    async fn test_on_event_invalid() {
        let connector = test_connector();
        let handler: EventHandler = Box::new(|_| {});
        assert!(connector.on_event("nonexistent", handler).await.is_err());
    }

    #[tokio::test]
    async fn test_dispatch_webhook() {
        let connector = test_connector();
        let received = Arc::new(Mutex::new(false));
        let received_clone = received.clone();

        let handler: EventHandler = Box::new(move |_| {
            if let Ok(mut r) = received_clone.try_lock() {
                *r = true;
            }
        });

        connector.on_event("chat_message", handler).await.unwrap();
        connector
            .dispatch_webhook(
                "chat_message",
                serde_json::json!({"sender": "user1", "content": "hello"}),
            )
            .await;

        assert!(*received.lock().await);
    }

    #[test]
    fn test_data_disclosure() {
        let connector = test_connector();
        assert_eq!(connector.manifest().data_disclosure.len(), 2);
    }
}
