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
use crate::client::GithubClient;
use crate::config::GithubConfig;
use crate::triggers;

/// GitHub connector.
///
/// Provides GitHub REST API v3 access with PAT authentication and
/// HMAC-SHA256 webhook verification. Triggers are driven by incoming
/// webhooks (handled externally by springtaled's management API).
pub struct GithubConnector {
    client: GithubClient,
    manifest: ConnectorManifest,
    triggers: Vec<TriggerDecl>,
    actions: Vec<ActionDecl>,
    /// Registered event handlers for webhook-driven triggers.
    handlers: Arc<Mutex<Vec<(SubscriptionId, String, EventHandler)>>>,
    /// Webhook secret for HMAC-SHA256 signature verification.
    webhook_secret: Option<secrecy::SecretBox<String>>,
    sub_counter: SubscriptionCounter,
}

impl GithubConnector {
    /// Create a new GitHub connector from config.
    pub fn new(config: GithubConfig) -> Result<Self, crate::error::GithubError> {
        let trigger_decls = triggers::trigger_declarations();
        let action_decls = actions::action_declarations();
        let manifest = build_manifest(&trigger_decls, &action_decls);
        // SECURITY: expose needed to clone webhook secret into connector's own SecretBox
        let webhook_secret = config.webhook_secret.as_ref().map(|s| {
            secrecy::SecretBox::new(Box::new(secrecy::ExposeSecret::expose_secret(s).clone()))
        });
        let client = GithubClient::new(&config)?;

        Ok(Self {
            client,
            manifest,
            triggers: trigger_decls,
            actions: action_decls,
            handlers: Arc::new(Mutex::new(Vec::new())),
            webhook_secret,
            sub_counter: SubscriptionCounter::new(),
        })
    }

    /// Dispatch a webhook event to registered handlers.
    ///
    /// Called by the management API when a GitHub webhook is received
    /// and signature-verified. The trigger name is derived from the
    /// `X-GitHub-Event` header.
    pub async fn dispatch_webhook(&self, trigger_name: &str, payload: serde_json::Value) {
        let handlers = self.handlers.lock().await;
        for (_id, registered, handler) in handlers.iter() {
            if registered == trigger_name {
                handler(payload.clone());
            }
        }
    }
}

#[async_trait]
impl Connector for GithubConnector {
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
            "create_issue" => actions::create_issue::execute(&self.client, &input)
                .await
                .map_err(ConnectorError::from),
            "post_comment" => actions::post_comment::execute(&self.client, &input)
                .await
                .map_err(ConnectorError::from),
            "get_diff" => actions::get_diff::execute(&self.client, &input)
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
            "push",
            "pull_request_opened",
            "issue_opened",
            "issue_comment",
        ];
        if !valid_triggers.contains(&trigger) {
            return Err(ConnectorError::ExecutionFailed(format!(
                "unknown trigger: {trigger}"
            )));
        }

        let id = self.sub_counter.next();
        let mut handlers = self.handlers.lock().await;
        handlers.push((id, trigger.to_owned(), handler));

        tracing::info!(trigger = trigger, "registered GitHub event handler");
        Ok(Subscription {
            id,
            trigger: trigger.to_owned(),
        })
    }

    async fn remove_event(&self, sub: &Subscription) -> Result<(), ConnectorError> {
        let mut handlers = self.handlers.lock().await;
        handlers.retain(|(id, _, _)| *id != sub.id);
        tracing::info!(id = ?sub.id, trigger = %sub.trigger, "removed GitHub event handler");
        Ok(())
    }

    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    async fn verify_webhook(
        &self,
        headers: &std::collections::HashMap<String, String>,
        body: &[u8],
    ) -> Result<(), ConnectorError> {
        let secret = self.webhook_secret.as_ref().ok_or_else(|| {
            ConnectorError::ExecutionFailed(
                "webhook_secret not configured — cannot verify GitHub webhook".to_owned(),
            )
        })?;

        let signature = headers.get("x-hub-signature-256").ok_or_else(|| {
            ConnectorError::ExecutionFailed("missing X-Hub-Signature-256 header".to_owned())
        })?;

        crate::webhook::verify_signature(secret, body, signature).map_err(|e| {
            ConnectorError::ExecutionFailed(format!("webhook verification failed: {e}"))
        })
    }
}

fn build_manifest(triggers: &[TriggerDecl], actions: &[ActionDecl]) -> ConnectorManifest {
    ConnectorManifest {
        name: "connector-github".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        author: "Springtale".to_owned(),
        description: "GitHub connector — REST API v3 with PAT auth and webhook verification."
            .to_owned(),
        capabilities: vec![Capability::NetworkOutbound {
            host: "api.github.com".to_owned(),
        }],
        triggers: triggers.to_vec(),
        actions: actions.to_vec(),
        data_disclosure: vec![
            DataDisclosure {
                data_type: "repository metadata".to_owned(),
                purpose: "creating issues, posting comments, fetching diffs".to_owned(),
                destination: "api.github.com".to_owned(),
            },
            DataDisclosure {
                data_type: "webhook payloads".to_owned(),
                purpose: "receiving push/PR/issue events for automation triggers".to_owned(),
                destination: "local only".to_owned(),
            },
        ],
        roles: vec![],
        wasm_hash: None,
        signature: None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use secrecy::SecretBox;

    fn test_connector() -> GithubConnector {
        let config = GithubConfig {
            token: SecretBox::new(Box::new("ghp_test".to_owned())),
            webhook_secret: None,
            api_base: "https://api.github.com".to_owned(),
        };
        GithubConnector::new(config).unwrap()
    }

    #[test]
    fn test_manifest_name() {
        let connector = test_connector();
        assert_eq!(connector.manifest().name, "connector-github");
    }

    #[test]
    fn test_manifest_network_capability() {
        let connector = test_connector();
        let has_github =
            connector.manifest().capabilities.iter().any(
                |c| matches!(c, Capability::NetworkOutbound { host } if host == "api.github.com"),
            );
        assert!(has_github);
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
        let names: Vec<&str> = connector
            .actions()
            .iter()
            .map(|a| a.name.as_str())
            .collect();
        assert!(names.contains(&"create_issue"));
        assert!(names.contains(&"post_comment"));
        assert!(names.contains(&"get_diff"));
    }

    #[tokio::test]
    async fn test_execute_unknown_action() {
        let connector = test_connector();
        let result = connector.execute("merge", serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_on_event_valid_trigger() {
        let connector = test_connector();
        let handler: EventHandler = Box::new(|_| {});
        let result = connector.on_event("push", handler).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_on_event_invalid_trigger() {
        let connector = test_connector();
        let handler: EventHandler = Box::new(|_| {});
        let result = connector.on_event("nonexistent", handler).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_dispatch_webhook() {
        let connector = test_connector();
        let received = Arc::new(Mutex::new(false));
        let received_clone = received.clone();

        let handler: EventHandler = Box::new(move |_payload| {
            if let Ok(mut r) = received_clone.try_lock() {
                *r = true;
            }
        });

        connector.on_event("push", handler).await.unwrap();
        connector
            .dispatch_webhook("push", serde_json::json!({"ref": "refs/heads/main"}))
            .await;

        let was_received = *received.lock().await;
        assert!(was_received);
    }

    #[test]
    fn test_data_disclosure() {
        let connector = test_connector();
        assert_eq!(connector.manifest().data_disclosure.len(), 2);
    }
}
