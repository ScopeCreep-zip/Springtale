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
use crate::client::DiscordClient;
use crate::config::DiscordConfig;
use crate::triggers;
use springtale_connector::manifest::SignatureAlgorithm;

/// Discord connector.
///
/// WARNING: Discord is a centralized platform that complies with
/// government data requests, including DHS subpoenas targeting
/// immigrant communities. Server admins can read ALL channels
/// (including "private" ones). IP addresses are logged on every
/// connection. Use a VPN.
///
/// Default mode: slash commands only (no MESSAGE_CONTENT intent).
/// This means the bot ONLY receives commands explicitly directed at it,
/// not all channel messages. Enable `enable_message_content` only if
/// you understand and accept the privacy cost.
///
/// Voice channel join is intentionally NOT implemented. The phase-2a spec
/// lists "voice channel join for presence" but joining voice channels
/// exposes the bot's presence to all channel members — observers can see
/// when the bot is "listening." For our target users (activists, IPV
/// survivors), this is an unacceptable privacy risk. Voice state can be
/// monitored passively via VoiceStateUpdate events without joining.
pub struct DiscordConnector {
    client: DiscordClient,
    manifest: ConnectorManifest,
    triggers: Vec<TriggerDecl>,
    actions: Vec<ActionDecl>,
    handlers: Arc<Mutex<Vec<(SubscriptionId, String, EventHandler)>>>,
    sub_counter: SubscriptionCounter,
}

impl DiscordConnector {
    /// Create a new DiscordConnector.
    pub fn new(config: &DiscordConfig) -> Result<Self, crate::error::DiscordError> {
        let trigger_decls = triggers::trigger_declarations();
        let action_decls = actions::action_declarations();

        // SECURITY: expose needed for twilight HTTP client initialization.
        // twilight_http::Client takes an owned String — cannot use Secret<T> directly.
        let token = secrecy::ExposeSecret::expose_secret(&config.bot_token).clone();

        // Validate token format before passing to twilight
        crate::auth::validate_bot_token(&token)?;

        let client = DiscordClient::new(token, config.message_jitter_secs);

        let manifest = build_manifest(&trigger_decls, &action_decls);

        Ok(Self {
            client,
            manifest,
            triggers: trigger_decls,
            actions: action_decls,
            handlers: Arc::new(Mutex::new(Vec::new())),
            sub_counter: SubscriptionCounter::new(),
        })
    }

    /// Access the underlying client (for wiring/gateway use).
    pub fn client(&self) -> &DiscordClient {
        &self.client
    }
}

#[async_trait]
impl Connector for DiscordConnector {
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
            "send_embed" => actions::send_embed::execute(&self.client, &input)
                .await
                .map_err(ConnectorError::from),
            "edit_message" => actions::edit_message::execute(&self.client, &input)
                .await
                .map_err(ConnectorError::from),
            "delete_message" => actions::delete_message::execute(&self.client, &input)
                .await
                .map_err(ConnectorError::from),
            "add_reaction" => actions::add_reaction::execute(&self.client, &input)
                .await
                .map_err(ConnectorError::from),
            "discover_destinations" => {
                actions::discover_destinations::execute(&self.client, &input)
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
            "interaction_received",
            "message_received",
            "dm_received",
            "reaction_added",
            "member_joined",
        ];
        if !valid.contains(&trigger) {
            return Err(ConnectorError::ExecutionFailed(format!(
                "unknown trigger: {trigger}"
            )));
        }

        let id = self.sub_counter.next();
        let mut handlers = self.handlers.lock().await;
        handlers.push((id, trigger.to_owned(), handler));
        tracing::info!(trigger = trigger, "registered Discord event handler");
        Ok(Subscription {
            id,
            trigger: trigger.to_owned(),
        })
    }

    async fn remove_event(&self, sub: &Subscription) -> Result<(), ConnectorError> {
        let mut handlers = self.handlers.lock().await;
        handlers.retain(|(id, _, _)| *id != sub.id);
        tracing::info!(id = ?sub.id, trigger = %sub.trigger, "removed Discord event handler");
        Ok(())
    }

    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    fn mention_extractor(&self) -> Option<&dyn springtale_connector::mention::MentionExtractor> {
        Some(&crate::mention::DISCORD_MENTION_EXTRACTOR)
    }
}

/// Build the connector's manifest. The factory calls this with no config-derived
/// parts so the manifest is available without instantiating the connector.
pub(crate) fn build_manifest(
    triggers: &[TriggerDecl],
    actions: &[ActionDecl],
) -> ConnectorManifest {
    ConnectorManifest {
        name: "connector-discord".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        author: "Springtale".to_owned(),
        description: "Discord connector — slash commands, messaging. \
                     WARNING: Discord complies with government data requests."
            .to_owned(),
        capabilities: vec![
            Capability::NetworkOutbound {
                host: "discord.com".to_owned(),
            },
            Capability::NetworkOutbound {
                host: "gateway.discord.gg".to_owned(),
            },
        ],
        triggers: triggers.to_vec(),
        actions: actions.to_vec(),
        data_disclosure: vec![
            DataDisclosure {
                data_type: "all messages and interactions".to_owned(),
                purpose: "messaging and slash commands".to_owned(),
                destination: "Discord servers (discord.com) — Discord retains data \
                             indefinitely and complies with government data requests \
                             including DHS subpoenas"
                    .to_owned(),
            },
            DataDisclosure {
                data_type: "bot token and connection metadata".to_owned(),
                purpose: "authentication and gateway connection".to_owned(),
                destination: "Discord API (discord.com, gateway.discord.gg) — \
                             IP address logged on every connection"
                    .to_owned(),
            },
            DataDisclosure {
                data_type: "guild membership and channel access".to_owned(),
                purpose: "receiving events from joined servers".to_owned(),
                destination: "Discord — server admins can see ALL channels \
                             including 'private' ones"
                    .to_owned(),
            },
            DataDisclosure {
                data_type: "message content (if MESSAGE_CONTENT intent enabled)".to_owned(),
                purpose: "reading messages for automation triggers".to_owned(),
                destination: "Discord API — bot reads ALL messages in ALL channels \
                             it can access. Disable enable_message_content and use \
                             slash commands instead for privacy."
                    .to_owned(),
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

    #[test]
    fn test_trigger_count() {
        assert_eq!(triggers::trigger_declarations().len(), 5);
    }

    #[test]
    fn test_action_count() {
        // 5 messaging actions + D1's `discover_destinations` enumeration.
        assert_eq!(actions::action_declarations().len(), 6);
    }
}
