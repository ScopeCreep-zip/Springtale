use springtale_connector::error::ConnectorError;
use springtale_connector::factory::{ConnectorFactory, FactoryEntry};
use springtale_connector::manifest::types::{ActionDecl, TriggerDecl};
use springtale_connector::Connector;

struct DiscordFactory;

#[async_trait::async_trait]
impl ConnectorFactory for DiscordFactory {
    fn name(&self) -> &'static str {
        "connector-discord"
    }
    fn config_key(&self) -> &'static str {
        "discord"
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
                    "description": "Discord bot token from Developer Portal",
                    "x-secret": true
                },
                "application_id": {
                    "type": "integer",
                    "description": "Discord application ID"
                },
                "guild_id": {
                    "type": "integer",
                    "description": "Guild ID for scoped slash commands (omit for global)"
                },
                "enable_message_content": {
                    "type": "boolean",
                    "description": "Allow reading all messages (privacy: exposes channel content)",
                    "default": false
                },
                "enable_direct_messages": {
                    "type": "boolean",
                    "description": "Enable DM triggers",
                    "default": false
                },
                "enable_reactions": {
                    "type": "boolean",
                    "description": "Enable reaction triggers",
                    "default": false
                },
                "message_jitter_secs": {
                    "type": "integer",
                    "description": "Publish-side jitter in seconds",
                    "default": 0
                }
            },
            "required": ["bot_token", "application_id"]
        }))
    }
    async fn create(
        &self,
        config: serde_json::Value,
    ) -> Result<Box<dyn Connector>, ConnectorError> {
        let config: crate::DiscordConfig = serde_json::from_value(config)
            .map_err(|e| ConnectorError::Serialization(e.to_string()))?;
        let connector = crate::DiscordConnector::new(&config)
            .map_err(|e| ConnectorError::ExecutionFailed(e.to_string()))?;
        Ok(Box::new(connector))
    }
}

inventory::submit!(FactoryEntry {
    factory: &DiscordFactory,
});
