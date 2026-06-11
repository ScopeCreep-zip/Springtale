use springtale_connector::Connector;
use springtale_connector::error::ConnectorError;
use springtale_connector::factory::{ConnectorFactory, FactoryEntry};
use springtale_connector::manifest::types::ActionDecl;

struct OpenCodeFactory;

#[async_trait::async_trait]
impl ConnectorFactory for OpenCodeFactory {
    fn name(&self) -> &'static str {
        "connector-opencode"
    }
    fn config_key(&self) -> &'static str {
        "opencode"
    }
    fn action_declarations(&self) -> Vec<ActionDecl> {
        crate::actions::action_declarations()
    }
    fn config_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "object",
            "properties": {
                "base_url": {
                    "type": "string",
                    "description": "Base URL of the running `opencode serve` daemon.",
                    "default": "http://127.0.0.1:4096"
                },
                "password": {
                    "type": "string",
                    "description": "Daemon basic-auth password (OPENCODE_SERVER_PASSWORD). Omit if the daemon runs without auth.",
                    "format": "password"
                },
                "model": {
                    "type": "string",
                    "description": "Optional model id to pass on each prompt (e.g. anthropic/claude-sonnet-4)."
                },
                "agent": {
                    "type": "string",
                    "description": "Optional opencode agent name to route prompts to."
                }
            },
            "required": []
        }))
    }
    async fn create(
        &self,
        config: serde_json::Value,
    ) -> Result<Box<dyn Connector>, ConnectorError> {
        let config: crate::OpenCodeConfig = serde_json::from_value(config)
            .map_err(|e| ConnectorError::Serialization(e.to_string()))?;
        let connector = crate::OpenCodeConnector::new(config)
            .map_err(|e| ConnectorError::ExecutionFailed(e.to_string()))?;
        Ok(Box::new(connector))
    }
}

inventory::submit!(FactoryEntry {
    factory: &OpenCodeFactory,
});
