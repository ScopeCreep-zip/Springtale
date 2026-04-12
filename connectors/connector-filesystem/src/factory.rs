use springtale_connector::Connector;
use springtale_connector::error::ConnectorError;
use springtale_connector::factory::{ConnectorFactory, FactoryEntry};
use springtale_connector::manifest::types::{ActionDecl, TriggerDecl};

struct FilesystemFactory;

#[async_trait::async_trait]
impl ConnectorFactory for FilesystemFactory {
    fn name(&self) -> &'static str {
        "connector-filesystem"
    }
    fn config_key(&self) -> &'static str {
        "filesystem"
    }
    fn requires_config(&self) -> bool {
        false
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
                "watch_paths": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Directories to watch for changes",
                    "default": []
                },
                "read_paths": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Directories allowed for reading",
                    "default": []
                },
                "write_paths": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Directories allowed for writing",
                    "default": []
                },
                "debounce_ms": {
                    "type": "integer",
                    "description": "File change debounce in milliseconds",
                    "default": 500
                }
            },
            "required": []
        }))
    }
    async fn create(
        &self,
        config: serde_json::Value,
    ) -> Result<Box<dyn Connector>, ConnectorError> {
        let config: crate::FilesystemConfig = serde_json::from_value(config)
            .map_err(|e| ConnectorError::Serialization(e.to_string()))?;
        Ok(Box::new(crate::FilesystemConnector::new(config)))
    }
}

inventory::submit!(FactoryEntry {
    factory: &FilesystemFactory,
});
