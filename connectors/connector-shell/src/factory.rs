use springtale_connector::Connector;
use springtale_connector::error::ConnectorError;
use springtale_connector::factory::{ConnectorFactory, FactoryEntry};
use springtale_connector::manifest::types::ActionDecl;

struct ShellFactory;

#[async_trait::async_trait]
impl ConnectorFactory for ShellFactory {
    fn name(&self) -> &'static str {
        "connector-shell"
    }
    fn config_key(&self) -> &'static str {
        "shell"
    }
    fn requires_config(&self) -> bool {
        false
    }
    fn action_declarations(&self) -> Vec<ActionDecl> {
        crate::actions::action_declarations()
    }
    fn config_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "object",
            "properties": {
                "allowed_commands": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Commands the connector is allowed to execute",
                    "default": []
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Command execution timeout in seconds",
                    "default": 30
                },
                "working_directory": {
                    "type": "string",
                    "description": "Working directory for command execution"
                }
            },
            "required": []
        }))
    }
    async fn create(
        &self,
        config: serde_json::Value,
    ) -> Result<Box<dyn Connector>, ConnectorError> {
        let config: crate::ShellConfig = serde_json::from_value(config)
            .map_err(|e| ConnectorError::Serialization(e.to_string()))?;
        Ok(Box::new(crate::ShellConnector::new(config)))
    }
}

inventory::submit!(FactoryEntry {
    factory: &ShellFactory,
});
