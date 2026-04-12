use springtale_connector::Connector;
use springtale_connector::error::ConnectorError;
use springtale_connector::factory::{ConnectorFactory, FactoryEntry};
use springtale_connector::manifest::types::{ActionDecl, TriggerDecl};

struct SlackFactory;

#[async_trait::async_trait]
impl ConnectorFactory for SlackFactory {
    fn name(&self) -> &'static str {
        "connector-slack"
    }
    fn config_key(&self) -> &'static str {
        "slack"
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
                "bot_token": {
                    "type": "string",
                    "description": "Slack bot token (xoxb-...)",
                    "x-secret": true
                },
                "app_token": {
                    "type": "string",
                    "description": "Slack app-level token for Socket Mode (xapp-...)",
                    "x-secret": true
                },
                "message_jitter_secs": {
                    "type": "integer",
                    "description": "Publish-side jitter in seconds",
                    "default": 0
                }
            },
            "required": ["bot_token", "app_token"]
        }))
    }
    async fn create(
        &self,
        config: serde_json::Value,
    ) -> Result<Box<dyn Connector>, ConnectorError> {
        let config: crate::SlackConfig = serde_json::from_value(config)
            .map_err(|e| ConnectorError::Serialization(e.to_string()))?;
        let connector = crate::SlackConnector::new(&config)
            .map_err(|e| ConnectorError::ExecutionFailed(e.to_string()))?;
        Ok(Box::new(connector))
    }
}

inventory::submit!(FactoryEntry {
    factory: &SlackFactory,
});
