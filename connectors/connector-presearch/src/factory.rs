use springtale_connector::error::ConnectorError;
use springtale_connector::factory::{ConnectorFactory, FactoryEntry};
use springtale_connector::manifest::types::ActionDecl;
use springtale_connector::Connector;

struct PresearchFactory;

#[async_trait::async_trait]
impl ConnectorFactory for PresearchFactory {
    fn name(&self) -> &'static str {
        "connector-presearch"
    }
    fn config_key(&self) -> &'static str {
        "presearch"
    }
    fn action_declarations(&self) -> Vec<ActionDecl> {
        crate::actions::action_declarations()
    }
    fn config_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "object",
            "properties": {
                "api_key": {
                    "type": "string",
                    "description": "Presearch API key",
                    "x-secret": true
                },
                "api_base": {
                    "type": "string",
                    "description": "Presearch API base URL",
                    "default": "https://presearch.com"
                },
                "cache_ttl_secs": {
                    "type": "integer",
                    "description": "Search result cache TTL in seconds",
                    "default": 300
                },
                "allowed_scrape_hosts": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Hosts allowed for scraping results",
                    "default": []
                }
            },
            "required": ["api_key"]
        }))
    }
    async fn create(
        &self,
        config: serde_json::Value,
    ) -> Result<Box<dyn Connector>, ConnectorError> {
        let config: crate::PresearchConfig = serde_json::from_value(config)
            .map_err(|e| ConnectorError::Serialization(e.to_string()))?;
        let connector = crate::PresearchConnector::new(config)
            .map_err(|e| ConnectorError::ExecutionFailed(e.to_string()))?;
        Ok(Box::new(connector))
    }
}

inventory::submit!(FactoryEntry {
    factory: &PresearchFactory,
});
