use springtale_connector::error::ConnectorError;
use springtale_connector::factory::{ConnectorFactory, FactoryEntry};
use springtale_connector::manifest::types::{ActionDecl, TriggerDecl};
use springtale_connector::Connector;

struct IrcFactory;

#[async_trait::async_trait]
impl ConnectorFactory for IrcFactory {
    fn name(&self) -> &'static str {
        "connector-irc"
    }
    fn config_key(&self) -> &'static str {
        "irc"
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
                "server": {
                    "type": "string",
                    "description": "IRC server hostname (e.g. irc.libera.chat)"
                },
                "port": {
                    "type": "integer",
                    "description": "Server port",
                    "default": 6697
                },
                "use_tls": {
                    "type": "boolean",
                    "description": "Use TLS (must be true for production)",
                    "default": true
                },
                "nick": {
                    "type": "string",
                    "description": "Bot nickname"
                },
                "nickserv_password": {
                    "type": "string",
                    "description": "NickServ identification password",
                    "x-secret": true
                },
                "sasl_enabled": {
                    "type": "boolean",
                    "description": "Enable SASL PLAIN authentication",
                    "default": false
                },
                "channels": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Channels to auto-join on connect",
                    "default": []
                },
                "command_prefix": {
                    "type": "string",
                    "description": "Bot command prefix",
                    "default": "!"
                },
                "message_jitter_secs": {
                    "type": "integer",
                    "description": "Activity jitter in seconds",
                    "default": 15
                }
            },
            "required": ["server", "nick"]
        }))
    }
    async fn create(
        &self,
        config: serde_json::Value,
    ) -> Result<Box<dyn Connector>, ConnectorError> {
        let config: crate::IrcConfig = serde_json::from_value(config)
            .map_err(|e| ConnectorError::Serialization(e.to_string()))?;
        let connector = crate::IrcConnector::new(&config)
            .await
            .map_err(|e| ConnectorError::ExecutionFailed(e.to_string()))?;
        Ok(Box::new(connector))
    }
}

inventory::submit!(FactoryEntry {
    factory: &IrcFactory,
});
