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
use crate::client::{AtProtoClient, BlueskyApi};
use crate::triggers;

/// Bluesky connector.
///
/// Provides Bluesky/ATProto integration with session-based authentication,
/// post/reply/like/repost actions, and Jetstream-driven triggers.
pub struct BlueskyConnector {
    client: Arc<AtProtoClient>,
    manifest: ConnectorManifest,
    triggers: Vec<TriggerDecl>,
    actions: Vec<ActionDecl>,
    handlers: Arc<Mutex<Vec<(SubscriptionId, String, EventHandler)>>>,
    sub_counter: SubscriptionCounter,
}

impl BlueskyConnector {
    /// Create a new Bluesky connector.
    ///
    /// The `client` should already be authenticated via `AtProtoClient::new()`.
    pub fn new(client: AtProtoClient) -> Self {
        let trigger_decls = triggers::trigger_declarations();
        let action_decls = actions::action_declarations();
        let manifest = build_manifest(&trigger_decls, &action_decls);

        Self {
            client: Arc::new(client),
            manifest,
            triggers: trigger_decls,
            actions: action_decls,
            handlers: Arc::new(Mutex::new(Vec::new())),
            sub_counter: SubscriptionCounter::new(),
        }
    }

    /// Dispatch a Jetstream event to registered handlers.
    pub async fn dispatch_event(&self, trigger_name: &str, payload: serde_json::Value) {
        let handlers = self.handlers.lock().await;
        for (_id, registered, handler) in handlers.iter() {
            if registered == trigger_name {
                handler(payload.clone());
            }
        }
    }
}

#[async_trait]
impl Connector for BlueskyConnector {
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
        let client: &dyn BlueskyApi = self.client.as_ref();
        match action {
            "create_post" => actions::create_post::execute(client, &input)
                .await
                .map_err(ConnectorError::from),
            "reply" => actions::reply::execute(client, &input)
                .await
                .map_err(ConnectorError::from),
            "like" => actions::like::execute(client, &input)
                .await
                .map_err(ConnectorError::from),
            "repost" => actions::repost::execute(client, &input)
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
        let valid_triggers = ["mention", "follow", "like", "repost"];
        if !valid_triggers.contains(&trigger) {
            return Err(ConnectorError::ExecutionFailed(format!(
                "unknown trigger: {trigger}"
            )));
        }

        let id = self.sub_counter.next();
        let mut handlers = self.handlers.lock().await;
        handlers.push((id, trigger.to_owned(), handler));
        tracing::info!(trigger = trigger, "registered Bluesky event handler");
        Ok(Subscription {
            id,
            trigger: trigger.to_owned(),
        })
    }

    async fn remove_event(&self, sub: &Subscription) -> Result<(), ConnectorError> {
        let mut handlers = self.handlers.lock().await;
        handlers.retain(|(id, _, _)| *id != sub.id);
        tracing::info!(id = ?sub.id, trigger = %sub.trigger, "removed Bluesky event handler");
        Ok(())
    }

    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }
}

fn build_manifest(triggers: &[TriggerDecl], actions: &[ActionDecl]) -> ConnectorManifest {
    ConnectorManifest {
        name: "connector-bluesky".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        author: "Springtale".to_owned(),
        description: "Bluesky connector — ATProto session auth, posts, replies, likes, reposts."
            .to_owned(),
        capabilities: vec![
            Capability::NetworkOutbound {
                host: "bsky.social".to_owned(),
            },
            Capability::NetworkOutbound {
                host: "jetstream2.us-west.bsky.network".to_owned(),
            },
        ],
        triggers: triggers.to_vec(),
        actions: actions.to_vec(),
        data_disclosure: vec![
            DataDisclosure {
                data_type: "post content".to_owned(),
                purpose: "creating posts, replies, likes, and reposts on Bluesky".to_owned(),
                destination: "bsky.social".to_owned(),
            },
            DataDisclosure {
                data_type: "firehose events".to_owned(),
                purpose: "receiving real-time events for automation triggers".to_owned(),
                destination: "local only".to_owned(),
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

    // Note: We can't easily test the full connector without a real ATProto
    // session, so we test manifest/declaration correctness here.

    #[test]
    fn test_manifest_name() {
        let trigger_decls = triggers::trigger_declarations();
        let action_decls = actions::action_declarations();
        let manifest = build_manifest(&trigger_decls, &action_decls);
        assert_eq!(manifest.name, "connector-bluesky");
    }

    #[test]
    fn test_manifest_capabilities() {
        let trigger_decls = triggers::trigger_declarations();
        let action_decls = actions::action_declarations();
        let manifest = build_manifest(&trigger_decls, &action_decls);

        let hosts: Vec<&str> = manifest
            .capabilities
            .iter()
            .filter_map(|c| match c {
                Capability::NetworkOutbound { host } => Some(host.as_str()),
                _ => None,
            })
            .collect();
        assert!(hosts.contains(&"bsky.social"));
        assert!(hosts.contains(&"jetstream2.us-west.bsky.network"));
    }

    #[test]
    fn test_four_triggers() {
        let triggers = triggers::trigger_declarations();
        assert_eq!(triggers.len(), 4);
    }

    #[test]
    fn test_four_actions() {
        let actions = actions::action_declarations();
        assert_eq!(actions.len(), 4);
        let names: Vec<&str> = actions.iter().map(|a| a.name.as_str()).collect();
        assert!(names.contains(&"create_post"));
        assert!(names.contains(&"reply"));
        assert!(names.contains(&"like"));
        assert!(names.contains(&"repost"));
    }

    #[test]
    fn test_data_disclosure() {
        let trigger_decls = triggers::trigger_declarations();
        let action_decls = actions::action_declarations();
        let manifest = build_manifest(&trigger_decls, &action_decls);
        assert_eq!(manifest.data_disclosure.len(), 2);
    }
}
