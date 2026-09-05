use async_trait::async_trait;

use springtale_connector::Subscription;
use springtale_connector::connector::trait_::{ActionResult, Connector, EventHandler};
use springtale_connector::error::ConnectorError;
use springtale_connector::manifest::SignatureAlgorithm;
use springtale_connector::manifest::types::{
    ActionDecl, Capability, ConnectorManifest, DataDisclosure, TriggerDecl,
};

use crate::actions;
use crate::client::OpenCodeClient;
use crate::config::OpenCodeConfig;

/// OpenCode connector — action-only, no triggers. All work flows to a local
/// `opencode serve` daemon over loopback HTTP.
pub struct OpenCodeConnector {
    client: OpenCodeClient,
    manifest: ConnectorManifest,
    actions: Vec<ActionDecl>,
}

impl OpenCodeConnector {
    pub fn new(config: OpenCodeConfig) -> Result<Self, crate::error::OpenCodeError> {
        let action_decls = actions::action_declarations();
        let manifest = build_manifest(config_capabilities(&config), &action_decls);
        let client = OpenCodeClient::new(config)?;
        Ok(Self {
            client,
            manifest,
            actions: action_decls,
        })
    }
}

#[async_trait]
impl Connector for OpenCodeConnector {
    fn triggers(&self) -> &[TriggerDecl] {
        &[]
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
            "run_task" => actions::run_task::execute(&self.client, &input)
                .await
                .map_err(ConnectorError::from),
            "continue_session" => actions::continue_session::execute(&self.client, &input)
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
        _handler: EventHandler,
    ) -> Result<Subscription, ConnectorError> {
        Err(ConnectorError::ExecutionFailed(format!(
            "OpenCode connector has no triggers, cannot register handler for: {trigger}"
        )))
    }

    async fn remove_event(&self, _sub: &Subscription) -> Result<(), ConnectorError> {
        Ok(())
    }

    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }
}

/// Parse the host:port out of the base URL for the NetworkOutbound
/// capability (exact-host match, no wildcards). Falls back to the
/// documented default loopback host on a malformed URL.
fn outbound_host(base_url: &str) -> String {
    base_url
        .split("://")
        .nth(1)
        .unwrap_or(base_url)
        .trim_end_matches('/')
        .split('/')
        .next()
        .unwrap_or("127.0.0.1:4096")
        .to_owned()
}

fn config_capabilities(config: &OpenCodeConfig) -> Vec<Capability> {
    vec![Capability::NetworkOutbound {
        host: outbound_host(&config.base_url),
    }]
}

/// Build the connector's manifest. The factory calls this with no config-derived
/// parts so the manifest is available without instantiating the connector.
pub(crate) fn build_manifest(
    capabilities: Vec<Capability>,
    actions: &[ActionDecl],
) -> ConnectorManifest {
    ConnectorManifest {
        name: "connector-opencode".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        author: "Springtale".to_owned(),
        description:
            "Drive a local `opencode serve` daemon for agentic coding tasks over its HTTP API."
                .to_owned(),
        capabilities,
        triggers: vec![],
        actions: actions.to_vec(),
        data_disclosure: vec![DataDisclosure {
            data_type: "coding task prompts and repository context".to_owned(),
            purpose: "performing agentic coding tasks via a local opencode daemon".to_owned(),
            destination: "the local opencode serve daemon".to_owned(),
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

    fn test_connector() -> OpenCodeConnector {
        let config: OpenCodeConfig = serde_json::from_value(serde_json::json!({})).unwrap();
        OpenCodeConnector::new(config).unwrap()
    }

    #[test]
    fn manifest_name_and_loopback_capability() {
        let connector = test_connector();
        assert_eq!(connector.manifest().name, "connector-opencode");
        let has_loopback =
            connector.manifest().capabilities.iter().any(
                |c| matches!(c, Capability::NetworkOutbound { host } if host == "127.0.0.1:4096"),
            );
        assert!(has_loopback);
    }

    #[test]
    fn declares_two_actions_both_mutating() {
        let connector = test_connector();
        assert_eq!(connector.actions().len(), 2);
        let names: Vec<&str> = connector
            .actions()
            .iter()
            .map(|a| a.name.as_str())
            .collect();
        assert!(names.contains(&"run_task"));
        assert!(names.contains(&"continue_session"));
        // Coding actions must never be advertised as read-only.
        assert!(connector.actions().iter().all(|a| !a.read_only));
    }

    #[test]
    fn no_triggers() {
        assert!(test_connector().triggers().is_empty());
    }

    #[test]
    fn outbound_host_parses_custom_url() {
        assert_eq!(outbound_host("http://localhost:5000/"), "localhost:5000");
        assert_eq!(outbound_host("http://127.0.0.1:4096"), "127.0.0.1:4096");
    }

    #[tokio::test]
    async fn unknown_action_errors() {
        let connector = test_connector();
        assert!(
            connector
                .execute("delete", serde_json::json!({}))
                .await
                .is_err()
        );
    }
}
