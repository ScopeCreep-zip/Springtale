use springtale_connector::Connector;
use springtale_connector::error::ConnectorError;
use springtale_connector::factory::{ConnectorFactory, FactoryEntry};
use springtale_connector::manifest::types::{ActionDecl, ConnectorManifest};

struct HttpFactory;

#[async_trait::async_trait]
impl ConnectorFactory for HttpFactory {
    fn name(&self) -> &'static str {
        "connector-http"
    }
    fn config_key(&self) -> &'static str {
        "http"
    }
    // Config-derived capabilities (allow-list) are omitted: the factory has no config.
    fn manifest(&self) -> ConnectorManifest {
        crate::connector::build_manifest(Vec::new(), &crate::actions::action_declarations())
    }
    fn action_declarations(&self) -> Vec<ActionDecl> {
        crate::actions::action_declarations()
    }
    fn config_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "object",
            "properties": {
                "allowed_hosts": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Hosts the connector may contact (exact match, no wildcards)",
                    "default": []
                },
                "default_headers": {
                    "type": "object",
                    "additionalProperties": { "type": "string" },
                    "description": "Default headers for every request (e.g. User-Agent)",
                    "default": {}
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Request timeout in seconds",
                    "default": 30
                }
            },
            "required": []
        }))
    }
    async fn create(
        &self,
        config: serde_json::Value,
    ) -> Result<Box<dyn Connector>, ConnectorError> {
        let config: crate::HttpConfig = serde_json::from_value(config)
            .map_err(|e| ConnectorError::Serialization(e.to_string()))?;
        let connector = crate::HttpConnector::new(config)
            .map_err(|e| ConnectorError::ExecutionFailed(e.to_string()))?;
        Ok(Box::new(connector))
    }
}

inventory::submit!(FactoryEntry {
    factory: &HttpFactory,
});
