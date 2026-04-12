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
use crate::client::SignalClient;
use crate::config::SignalConfig;
use crate::triggers;

/// Signal connector — signal-cli bridge, E2E encrypted messaging.
///
/// Bridges to a signal-cli daemon running as a separate process.
/// All Signal Protocol operations (encryption, key exchange, registration)
/// are handled by signal-cli. Springtale communicates via HTTP JSON-RPC.
///
/// PRIVACY:
/// - E2E encrypted — Signal server cannot read message content
/// - Phone number stored in signal-cli only, NOT in Springtale config
/// - signal-cli stores message DB and keys in PLAINTEXT on local disk
/// - For device seizure protection: full-disk encryption + --ephemeral mode
/// - Disappearing messages: reliable in 1:1, limited in groups (signal-cli limitation)
pub struct SignalConnector {
    client: SignalClient,
    manifest: ConnectorManifest,
    triggers: Vec<TriggerDecl>,
    actions: Vec<ActionDecl>,
    handlers: Arc<Mutex<Vec<(SubscriptionId, String, EventHandler)>>>,
    sub_counter: SubscriptionCounter,
}

impl SignalConnector {
    pub fn new(config: &SignalConfig) -> Result<Self, crate::error::SignalError> {
        let trigger_decls = triggers::trigger_declarations();
        let action_decls = actions::action_declarations();

        crate::auth::validate_daemon_url(&config.daemon_url)?;

        let client = SignalClient::new(
            config.daemon_url.clone(),
            config.account_id.clone(),
            config.message_jitter_secs,
        );

        let manifest = ConnectorManifest {
            name: "connector-signal".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            author: "Springtale".to_owned(),
            description: "Signal connector — signal-cli bridge, E2E encrypted messaging, \
                         disappearing messages."
                .to_owned(),
            capabilities: vec![Capability::NetworkOutbound {
                host: "localhost".to_owned(),
            }],
            triggers: trigger_decls.clone(),
            actions: action_decls.clone(),
            data_disclosure: vec![
                DataDisclosure {
                    data_type: "message content (E2E encrypted in transit)".to_owned(),
                    purpose: "sending and receiving Signal messages".to_owned(),
                    destination: "Signal server sees ciphertext only. Content readable \
                                 only by sender and recipient."
                        .to_owned(),
                },
                DataDisclosure {
                    data_type: "signal-cli local data (message DB, encryption keys)".to_owned(),
                    purpose: "Signal Protocol session management".to_owned(),
                    destination: "local disk (~/.local/share/signal-cli/data/) — stored \
                                 in PLAINTEXT. Anyone with filesystem access can read \
                                 all messages and clone the account."
                        .to_owned(),
                },
                DataDisclosure {
                    data_type: "phone number".to_owned(),
                    purpose: "Signal account registration".to_owned(),
                    destination: "stored in signal-cli local data ONLY, NOT in Springtale \
                                 config. Signal server knows the number."
                        .to_owned(),
                },
                DataDisclosure {
                    data_type: "connection metadata (IP, timestamps)".to_owned(),
                    purpose: "Signal Protocol transport".to_owned(),
                    destination: "Signal server logs connection metadata. \
                                 Use a VPN to protect IP address."
                        .to_owned(),
                },
            ],
            wasm_hash: None,
            signature: None,
        };

        Ok(Self {
            client,
            manifest,
            triggers: trigger_decls,
            actions: action_decls,
            handlers: Arc::new(Mutex::new(Vec::new())),
            sub_counter: SubscriptionCounter::new(),
        })
    }

    /// Access the client (for gateway wiring).
    pub fn client(&self) -> &SignalClient {
        &self.client
    }
}

#[async_trait]
impl Connector for SignalConnector {
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
            "send_message" => actions::send_message::execute(&self.client, &input)
                .await
                .map_err(ConnectorError::from),
            "send_group_message" => actions::send_group_message::execute(&self.client, &input)
                .await
                .map_err(ConnectorError::from),
            "set_disappearing_timer" => {
                actions::set_disappearing_timer::execute(&self.client, &input)
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
        let valid = [
            "message_received",
            "group_message_received",
            "disappearing_timer_changed",
        ];
        if !valid.contains(&trigger) {
            return Err(ConnectorError::ExecutionFailed(format!(
                "unknown trigger: {trigger}"
            )));
        }

        let id = self.sub_counter.next();
        let mut handlers = self.handlers.lock().await;
        handlers.push((id, trigger.to_owned(), handler));
        tracing::info!(trigger = trigger, "registered Signal event handler");
        Ok(Subscription {
            id,
            trigger: trigger.to_owned(),
        })
    }

    async fn remove_event(&self, sub: &Subscription) -> Result<(), ConnectorError> {
        let mut handlers = self.handlers.lock().await;
        handlers.retain(|(id, _, _)| *id != sub.id);
        tracing::info!(id = ?sub.id, trigger = %sub.trigger, "removed Signal event handler");
        Ok(())
    }

    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_count() {
        assert_eq!(triggers::trigger_declarations().len(), 3);
    }

    #[test]
    fn test_action_count() {
        assert_eq!(actions::action_declarations().len(), 3);
    }
}
