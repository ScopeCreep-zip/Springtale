use async_trait::async_trait;

use springtale_connector::connector::trait_::{ActionResult, Connector, EventHandler};
use springtale_connector::error::ConnectorError;
use springtale_connector::manifest::types::{
    ActionDecl, Capability, ConnectorManifest, DataDisclosure, TriggerDecl,
};

use crate::actions;
use crate::cache::ResultCache;
use crate::client::PresearchClient;
use crate::config::PresearchConfig;

/// Presearch connector.
///
/// Provides privacy-first web search and URL scraping via the Presearch
/// decentralized search engine. Action-only connector — no triggers.
/// Includes a TTL-based result cache to reduce redundant API calls.
pub struct PresearchConnector {
    client: PresearchClient,
    cache: ResultCache,
    manifest: ConnectorManifest,
    actions: Vec<ActionDecl>,
}

impl PresearchConnector {
    /// Create a new Presearch connector from config.
    pub fn new(config: PresearchConfig) -> Result<Self, crate::error::PresearchError> {
        let action_decls = actions::action_declarations();
        let manifest = build_manifest(&config, &action_decls);
        let cache = ResultCache::new(config.cache_ttl());
        let client = PresearchClient::new(&config)?;

        Ok(Self {
            client,
            cache,
            manifest,
            actions: action_decls,
        })
    }
}

#[async_trait]
impl Connector for PresearchConnector {
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
            "search" => actions::search::execute(&self.client, &self.cache, &input)
                .await
                .map_err(ConnectorError::from),
            "scrape" => actions::scrape::execute(&self.client, &self.cache, &input)
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
    ) -> Result<(), ConnectorError> {
        Err(ConnectorError::ExecutionFailed(format!(
            "Presearch connector has no triggers, cannot register handler for: {trigger}"
        )))
    }

    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }
}

fn build_manifest(
    config: &PresearchConfig,
    actions: &[ActionDecl],
) -> ConnectorManifest {
    let capabilities = config
        .all_network_hosts()
        .into_iter()
        .map(|host| Capability::NetworkOutbound { host })
        .collect();

    ConnectorManifest {
        name: "connector-presearch".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        author: "Springtale".to_owned(),
        description: "Privacy-first decentralized search via Presearch with result caching."
            .to_owned(),
        capabilities,
        triggers: vec![],
        actions: actions.to_vec(),
        data_disclosure: vec![DataDisclosure {
            data_type: "search queries".to_owned(),
            purpose: "executing web searches as directed by automation rules".to_owned(),
            destination: "presearch.com".to_owned(),
        }],
        wasm_hash: None,
        signature: None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use secrecy::SecretBox;

    fn test_connector() -> PresearchConnector {
        let config = PresearchConfig {
            api_key: SecretBox::new(Box::new("test_key".to_owned())),
            api_base: "https://presearch.com".to_owned(),
            cache_ttl_secs: 300, allowed_scrape_hosts: vec![],
        };
        PresearchConnector::new(config).unwrap()
    }

    #[test]
    fn test_manifest_name() {
        let connector = test_connector();
        assert_eq!(connector.manifest().name, "connector-presearch");
    }

    #[test]
    fn test_manifest_network_capability() {
        let connector = test_connector();
        let has_presearch = connector.manifest().capabilities.iter().any(|c| {
            matches!(c, Capability::NetworkOutbound { host } if host == "presearch.com")
        });
        assert!(has_presearch);
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
        let names: Vec<&str> = connector.actions().iter().map(|a| a.name.as_str()).collect();
        assert!(names.contains(&"search"));
        assert!(names.contains(&"scrape"));
    }

    #[tokio::test]
    async fn test_execute_unknown_action() {
        let connector = test_connector();
        let result = connector.execute("crawl", serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_on_event_rejected() {
        let connector = test_connector();
        let handler: EventHandler = Box::new(|_| {});
        let result = connector.on_event("anything", handler).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_data_disclosure() {
        let connector = test_connector();
        assert_eq!(connector.manifest().data_disclosure.len(), 1);
        assert_eq!(
            connector.manifest().data_disclosure[0].destination,
            "presearch.com"
        );
    }
}
