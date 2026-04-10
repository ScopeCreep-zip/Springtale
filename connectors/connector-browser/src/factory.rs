use springtale_connector::error::ConnectorError;
use springtale_connector::factory::{ConnectorFactory, FactoryEntry};
use springtale_connector::manifest::types::{ActionDecl, TriggerDecl};
use springtale_connector::Connector;

struct BrowserFactory;

#[async_trait::async_trait]
impl ConnectorFactory for BrowserFactory {
    fn name(&self) -> &'static str {
        "connector-browser"
    }
    fn config_key(&self) -> &'static str {
        "browser"
    }
    fn trigger_declarations(&self) -> Vec<TriggerDecl> {
        crate::triggers::trigger_declarations()
    }
    fn action_declarations(&self) -> Vec<ActionDecl> {
        crate::actions::action_declarations()
    }
    fn config_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "object",
            "properties": {
                "allowed_domains": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Domains allowed for navigation (exact match, no wildcards)"
                },
                "chrome_path": {
                    "type": "string",
                    "description": "Path to Chrome/Chromium binary (auto-detected if omitted)"
                },
                "disable_telemetry": {
                    "type": "boolean",
                    "description": "Disable Chrome telemetry",
                    "default": true
                },
                "message_jitter_secs": {
                    "type": "integer",
                    "description": "Publish-side jitter in seconds",
                    "default": 0
                }
            },
            "required": ["allowed_domains"]
        }))
    }
    async fn create(
        &self,
        config: serde_json::Value,
    ) -> Result<Box<dyn Connector>, ConnectorError> {
        let config: crate::BrowserConfig = serde_json::from_value(config)
            .map_err(|e| ConnectorError::Serialization(e.to_string()))?;
        let connector = crate::BrowserConnector::new(&config)
            .map_err(|e| ConnectorError::ExecutionFailed(e.to_string()))?;
        Ok(Box::new(connector))
    }
}

inventory::submit!(FactoryEntry {
    factory: &BrowserFactory,
});
