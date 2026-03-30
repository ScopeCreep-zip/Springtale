use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use springtale_connector::connector::trait_::{ActionResult, Connector, EventHandler};
use springtale_connector::error::ConnectorError;
use springtale_connector::manifest::types::{
    ActionDecl, Capability, ConnectorManifest, DataDisclosure, TriggerDecl,
};

use crate::actions;
use crate::client::KickClient;
use crate::config::KickConfig;
use crate::triggers;
use crate::webhook;

/// Kick connector.
///
/// Provides Kick platform integration with OAuth 2.1 PKCE authentication,
/// REST API actions, and webhook-driven triggers. When a trigger handler
/// is registered via `on_event()`, the connector subscribes with Kick's
/// event subscription API so webhooks are delivered.
pub struct KickConnector {
    client: KickClient,
    manifest: ConnectorManifest,
    triggers: Vec<TriggerDecl>,
    actions: Vec<ActionDecl>,
    handlers: Arc<Mutex<Vec<(String, EventHandler)>>>,
    /// Webhook callback URL where Kick sends events (from config).
    webhook_callback_url: Option<String>,
    /// Kick event types we've already subscribed to (avoid duplicate subscriptions).
    subscribed_events: Mutex<HashSet<String>>,
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
        let client = KickClient::new(&config.api_base, access_token)?;

        Ok(Self {
            client,
            manifest,
            triggers: trigger_decls,
            actions: action_decls,
            handlers: Arc::new(Mutex::new(Vec::new())),
            webhook_callback_url: config.webhook_callback_url.clone(),
            subscribed_events: Mutex::new(HashSet::new()),
        })
    }

    /// Dispatch a webhook event to registered handlers by trigger name.
    pub async fn dispatch_webhook(&self, trigger_name: &str, payload: serde_json::Value) {
        let handlers = self.handlers.lock().await;
        for (registered, handler) in handlers.iter() {
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
        match action {
            "send_chat" => actions::send_chat::execute(&self.client, &input)
                .await
                .map_err(ConnectorError::from),
            "get_channel" => actions::get_channel::execute(&self.client, &input)
                .await
                .map_err(ConnectorError::from),
            "get_stream" => actions::get_stream::execute(&self.client, &input)
                .await
                .map_err(ConnectorError::from),
            unknown => Err(ConnectorError::ExecutionFailed(format!(
                "unknown action: {unknown}"
            ))),
        }
    }

    async fn on_event(&self, trigger: &str, handler: EventHandler) -> Result<(), ConnectorError> {
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
        {
            let mut handlers = self.handlers.lock().await;
            handlers.push((trigger.to_owned(), handler));
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
        Ok(())
    }

    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }
}

fn build_manifest(triggers: &[TriggerDecl], actions: &[ActionDecl]) -> ConnectorManifest {
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
        wasm_hash: None,
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
