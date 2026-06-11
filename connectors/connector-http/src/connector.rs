use async_trait::async_trait;

use springtale_connector::Subscription;
use springtale_connector::connector::trait_::{ActionResult, Connector, EventHandler};
use springtale_connector::error::ConnectorError;
use springtale_connector::manifest::types::{
    ActionDecl, Capability, ConnectorManifest, DataDisclosure, TriggerDecl,
};

use crate::actions;
use crate::client::HttpClient;
use crate::config::HttpConfig;
use springtale_connector::manifest::SignatureAlgorithm;

/// Generic HTTP connector.
///
/// Makes GET/POST requests to allow-listed hosts. Action-only connector — no triggers.
/// All network calls go through the `HttpClient` which enforces host validation.
pub struct HttpConnector {
    client: HttpClient,
    manifest: ConnectorManifest,
    actions: Vec<ActionDecl>,
}

impl HttpConnector {
    /// Create a new HTTP connector with the given configuration.
    pub fn new(config: HttpConfig) -> Result<Self, crate::error::HttpError> {
        let action_decls = actions::action_declarations();
        let manifest = build_manifest(&config, &action_decls);
        let client = HttpClient::new(config)?;

        Ok(Self {
            client,
            manifest,
            actions: action_decls,
        })
    }
}

#[async_trait]
impl Connector for HttpConnector {
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
            "get" => actions::get::execute(&self.client, &input)
                .await
                .map_err(ConnectorError::from),
            "post" => actions::post::execute(&self.client, &input)
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
            "HTTP connector has no triggers, cannot register handler for: {trigger}"
        )))
    }

    async fn remove_event(&self, _sub: &Subscription) -> Result<(), ConnectorError> {
        Ok(())
    }

    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }
}

/// Build the connector manifest from config and declarations.
fn build_manifest(config: &HttpConfig, actions: &[ActionDecl]) -> ConnectorManifest {
    let capabilities = config
        .allowed_hosts
        .iter()
        .map(|host| Capability::NetworkOutbound { host: host.clone() })
        .collect();

    ConnectorManifest {
        name: "connector-http".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        author: "Springtale".to_owned(),
        description: "Generic HTTP connector — make GET/POST requests to allow-listed hosts."
            .to_owned(),
        capabilities,
        triggers: vec![],
        actions: actions.to_vec(),
        data_disclosure: vec![DataDisclosure {
            data_type: "HTTP request/response data".to_owned(),
            purpose: "making HTTP requests as directed by automation rules".to_owned(),
            destination: "configured allow-listed hosts".to_owned(),
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
    use std::collections::HashMap;

    fn test_connector() -> HttpConnector {
        let config = HttpConfig {
            allowed_hosts: vec!["api.example.com".to_owned()],
            default_headers: HashMap::new(),
            timeout_secs: 5,
        };
        HttpConnector::new(config).unwrap()
    }

    #[test]
    fn test_manifest_name() {
        let connector = test_connector();
        assert_eq!(connector.manifest().name, "connector-http");
    }

    #[test]
    fn test_manifest_network_capability() {
        let connector = test_connector();
        let has_network = connector.manifest().capabilities.iter().any(
            |c| matches!(c, Capability::NetworkOutbound { host } if host == "api.example.com"),
        );
        assert!(has_network);
    }

    #[test]
    fn test_no_triggers() {
        let connector = test_connector();
        assert!(connector.triggers().is_empty());
    }

    #[test]
    fn test_two_actions() {
        let connector = test_connector();
        assert_eq!(connector.actions().len(), 2);
        let names: Vec<&str> = connector
            .actions()
            .iter()
            .map(|a| a.name.as_str())
            .collect();
        assert!(names.contains(&"get"));
        assert!(names.contains(&"post"));
    }

    #[tokio::test]
    async fn test_execute_unknown_action() {
        let connector = test_connector();
        let result = connector.execute("delete", serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_on_event_rejected() {
        let connector = test_connector();
        let handler: EventHandler = Box::new(|_| {});
        let result = connector.on_event("anything", handler).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_host_not_allowed() {
        let connector = test_connector();
        let result = connector
            .execute("get", serde_json::json!({ "url": "https://evil.com/data" }))
            .await;

        assert!(result.is_err());
    }

    #[test]
    fn test_data_disclosure() {
        let connector = test_connector();
        let disclosures = &connector.manifest().data_disclosure;
        assert_eq!(disclosures.len(), 1);
    }
}
