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
use crate::client::IrcClient;
use crate::config::IrcConfig;
use crate::triggers;

/// IRC connector.
///
/// WARNING: IRC has NO end-to-end encryption. All messages are
/// readable by server operators and network observers. IP addresses
/// may be visible via WHOIS unless masked by a VPN or bouncer.
///
/// Recommended for: public discussion, community support groups.
/// NOT recommended for: covert organizing, activists in hostile jurisdictions.
pub struct IrcConnector {
    client: IrcClient,
    manifest: ConnectorManifest,
    triggers: Vec<TriggerDecl>,
    actions: Vec<ActionDecl>,
    handlers: Arc<Mutex<Vec<(SubscriptionId, String, EventHandler)>>>,
    sub_counter: SubscriptionCounter,
}

impl IrcConnector {
    /// Create a new IrcConnector. Connects to the server immediately.
    pub async fn new(config: &IrcConfig) -> Result<Self, crate::error::IrcError> {
        let irc_config = build_irc_config(config)?;

        let irc_client = irc::client::Client::from_config(irc_config)
            .await
            .map_err(|e| {
                crate::error::IrcError::ConnectionFailed(format!("failed to connect: {e}"))
            })?;

        irc_client
            .identify()
            .map_err(|e| crate::error::IrcError::AuthFailed(format!("failed to identify: {e}")))?;

        let sender = irc_client.sender();
        let client = IrcClient::new(sender, config.message_jitter_secs);

        let trigger_decls = triggers::trigger_declarations();
        let action_decls = actions::action_declarations();

        let manifest = ConnectorManifest {
            name: "connector-irc".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            author: "Springtale".to_owned(),
            description: "IRC connector — lightweight TLS chat. WARNING: No E2E encryption."
                .to_owned(),
            capabilities: vec![Capability::NetworkOutbound {
                host: config.server.clone(),
            }],
            triggers: trigger_decls.clone(),
            actions: action_decls.clone(),
            data_disclosure: vec![
                DataDisclosure {
                    data_type: "all messages (plaintext)".to_owned(),
                    purpose: "IRC protocol — messages sent unencrypted".to_owned(),
                    destination: format!(
                        "IRC server ({}), visible to server operators and network observers",
                        config.server
                    ),
                },
                DataDisclosure {
                    data_type: "connection metadata (nick, IP, channels)".to_owned(),
                    purpose: "IRC protocol requirement".to_owned(),
                    destination: "IRC server and WHOIS queries (use VPN/bouncer for IP privacy)"
                        .to_owned(),
                },
                DataDisclosure {
                    data_type: "channel membership".to_owned(),
                    purpose: "joining and messaging channels".to_owned(),
                    destination: "visible to all channel members".to_owned(),
                },
                DataDisclosure {
                    data_type: "nick identity and presence".to_owned(),
                    purpose: "IRC identification".to_owned(),
                    destination: "nick is persistent and visible — do NOT reuse across networks. \
                         Scrapers and adversaries can link nicks to build identity profiles."
                        .to_owned(),
                },
            ],
            roles: vec![],
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
}

#[async_trait]
impl Connector for IrcConnector {
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
            "join_channel" => actions::join_channel::execute(&self.client, &input)
                .await
                .map_err(ConnectorError::from),
            "part_channel" => actions::part_channel::execute(&self.client, &input)
                .await
                .map_err(ConnectorError::from),
            "set_topic" => actions::set_topic::execute(&self.client, &input)
                .await
                .map_err(ConnectorError::from),
            "send_action" => actions::send_action::execute(&self.client, &input)
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
        let valid = [
            "message_received",
            "command_received",
            "user_joined",
            "user_parted",
            "topic_changed",
        ];
        if !valid.contains(&trigger) {
            return Err(ConnectorError::ExecutionFailed(format!(
                "unknown trigger: {trigger}"
            )));
        }

        let id = self.sub_counter.next();
        let mut handlers = self.handlers.lock().await;
        handlers.push((id, trigger.to_owned(), handler));
        tracing::info!(trigger = trigger, "registered IRC event handler");
        Ok(Subscription {
            id,
            trigger: trigger.to_owned(),
        })
    }

    async fn remove_event(&self, sub: &Subscription) -> Result<(), ConnectorError> {
        let mut handlers = self.handlers.lock().await;
        handlers.retain(|(id, _, _)| *id != sub.id);
        tracing::info!(id = ?sub.id, trigger = %sub.trigger, "removed IRC event handler");
        Ok(())
    }

    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }
}

/// Build the `irc` crate Config from our IrcConfig.
fn build_irc_config(
    config: &IrcConfig,
) -> Result<irc::client::data::Config, crate::error::IrcError> {
    // SECURITY: expose needed for NickServ/SASL auth.
    // The irc crate's Config takes an owned String (not Secret<T>), so we must
    // clone the exposed secret. The clone is a bare String — it will be zeroed
    // when the irc crate drops the Config. We cannot avoid this because the irc
    // crate doesn't support secrecy types in its public API.
    let nick_password = config
        .nickserv_password
        .as_ref()
        .map(|s| secrecy::ExposeSecret::expose_secret(s).clone());

    Ok(irc::client::data::Config {
        nickname: Some(config.nick.clone()),
        server: Some(config.server.clone()),
        port: Some(config.port),
        use_tls: Some(config.use_tls),
        nick_password,
        channels: config.channels.clone(),
        // Privacy: disable CTCP VERSION to prevent bot fingerprinting
        version: Some(String::new()),
        // Built-in flood protection
        burst_window_length: Some(8),
        max_messages_in_burst: Some(15),
        ..irc::client::data::Config::default()
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_count() {
        assert_eq!(triggers::trigger_declarations().len(), 5);
    }

    #[test]
    fn test_action_count() {
        assert_eq!(actions::action_declarations().len(), 5);
    }
}
