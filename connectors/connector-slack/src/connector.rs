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
use crate::client::SlackClient;
use crate::config::SlackConfig;
use crate::triggers;
use springtale_connector::manifest::SignatureAlgorithm;

/// Slack connector — Socket Mode, slash commands, Block Kit, threads.
///
/// WARNING: Slack is enterprise software. This is possibly the MOST
/// hostile environment for vulnerable users:
///
/// - Workspace admins can read ALL messages including DMs
/// - No notification when data is exported (changed 2018)
/// - Enterprise Grid has full compliance export and eDiscovery
/// - Slack complies with government data requests
/// - Both tokens revocable by workspace admin without notice
/// - Data retention is admin-controlled — users CANNOT control
///   their own data retention
///
/// Do NOT use Slack for covert organizing, asylum coordination,
/// IPV safety planning, or anything you wouldn't show your employer.
/// Use Signal or Matrix for sensitive communications instead.
pub struct SlackConnector {
    client: SlackClient,
    manifest: ConnectorManifest,
    triggers: Vec<TriggerDecl>,
    actions: Vec<ActionDecl>,
    handlers: Arc<Mutex<Vec<(SubscriptionId, String, EventHandler)>>>,
    sub_counter: SubscriptionCounter,
}

impl SlackConnector {
    /// Create a new SlackConnector.
    pub fn new(config: &SlackConfig) -> Result<Self, crate::error::SlackError> {
        let trigger_decls = triggers::trigger_declarations();
        let action_decls = actions::action_declarations();

        // Validate token formats before proceeding
        {
            // SECURITY: expose needed for token format validation only
            let bot_token_str = secrecy::ExposeSecret::expose_secret(&config.bot_token);
            let app_token_str = secrecy::ExposeSecret::expose_secret(&config.app_token);
            crate::auth::validate_bot_token(bot_token_str)?;
            crate::auth::validate_app_token(app_token_str)?;
        }

        // SECURITY: expose needed to clone bot token into client's own SecretBox
        let bot_token = secrecy::SecretBox::new(Box::new(
            secrecy::ExposeSecret::expose_secret(&config.bot_token).clone(),
        ));
        let client = SlackClient::new(bot_token, config.message_jitter_secs);

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
}

#[async_trait]
impl Connector for SlackConnector {
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
            "send_blocks" => actions::send_blocks::execute(&self.client, &input)
                .await
                .map_err(ConnectorError::from),
            "send_thread_reply" => actions::send_thread_reply::execute(&self.client, &input)
                .await
                .map_err(ConnectorError::from),
            "edit_message" => actions::edit_message::execute(&self.client, &input)
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
            "slash_command",
            "message_received",
            "app_mentioned",
            "reaction_added",
            "thread_reply",
        ];
        if !valid.contains(&trigger) {
            return Err(ConnectorError::ExecutionFailed(format!(
                "unknown trigger: {trigger}"
            )));
        }

        let id = self.sub_counter.next();
        let mut handlers = self.handlers.lock().await;
        handlers.push((id, trigger.to_owned(), handler));
        tracing::info!(trigger = trigger, "registered Slack event handler");
        Ok(Subscription {
            id,
            trigger: trigger.to_owned(),
        })
    }

    async fn remove_event(&self, sub: &Subscription) -> Result<(), ConnectorError> {
        let mut handlers = self.handlers.lock().await;
        handlers.retain(|(id, _, _)| *id != sub.id);
        tracing::info!(id = ?sub.id, trigger = %sub.trigger, "removed Slack event handler");
        Ok(())
    }

    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    fn mention_extractor(&self) -> Option<&dyn springtale_connector::mention::MentionExtractor> {
        Some(&crate::mention::SLACK_MENTION_EXTRACTOR)
    }
}

/// Build the connector's manifest. The factory calls this with no config-derived
/// parts so the manifest is available without instantiating the connector.
pub(crate) fn build_manifest(
    triggers: &[TriggerDecl],
    actions: &[ActionDecl],
) -> ConnectorManifest {
    ConnectorManifest {
        name: "connector-slack".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        author: "Springtale".to_owned(),
        description: "Slack connector — Socket Mode, slash commands, Block Kit, threads. \
                     WARNING: Workspace admins can read ALL messages."
            .to_owned(),
        capabilities: vec![
            Capability::NetworkOutbound {
                host: "slack.com".to_owned(),
            },
            Capability::NetworkOutbound {
                host: "wss-primary.slack.com".to_owned(),
            },
        ],
        triggers: triggers.to_vec(),
        actions: actions.to_vec(),
        data_disclosure: vec![
            DataDisclosure {
                data_type: "all messages sent by bot".to_owned(),
                purpose: "messaging and slash command responses".to_owned(),
                destination: "Slack workspace — workspace admins can read ALL messages \
                             including DMs. No notification when data is exported \
                             (changed 2018). Slack complies with government data requests."
                    .to_owned(),
            },
            DataDisclosure {
                data_type: "bot tokens and connection metadata".to_owned(),
                purpose: "authentication and Socket Mode connection".to_owned(),
                destination: "Slack API (slack.com) — IP logged. Both tokens revocable \
                             by workspace admin at any time without notice."
                    .to_owned(),
            },
            DataDisclosure {
                data_type: "channel membership and message history".to_owned(),
                purpose: "receiving events from channels bot is in".to_owned(),
                destination: "visible to workspace admins and Enterprise Grid \
                             compliance/eDiscovery tools. Full export includes \
                             all conversations."
                    .to_owned(),
            },
            DataDisclosure {
                data_type: "message content in all channels bot is in".to_owned(),
                purpose: "automation triggers and command processing".to_owned(),
                destination: "admin-controlled retention. Users CANNOT control their \
                             own data retention. Do NOT use for sensitive organizing."
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
