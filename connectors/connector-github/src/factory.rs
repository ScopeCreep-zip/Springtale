use springtale_connector::error::ConnectorError;
use springtale_connector::factory::{ConnectorFactory, FactoryEntry};
use springtale_connector::manifest::types::{ActionDecl, TriggerDecl};
use springtale_connector::Connector;

struct GithubFactory;

#[async_trait::async_trait]
impl ConnectorFactory for GithubFactory {
    fn name(&self) -> &'static str {
        "connector-github"
    }
    fn config_key(&self) -> &'static str {
        "github"
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
                "token": {
                    "type": "string",
                    "description": "GitHub Personal Access Token",
                    "x-secret": true
                },
                "webhook_secret": {
                    "type": "string",
                    "description": "Webhook secret for HMAC-SHA256 verification",
                    "x-secret": true
                },
                "api_base": {
                    "type": "string",
                    "description": "GitHub API base URL",
                    "default": "https://api.github.com"
                }
            },
            "required": ["token"]
        }))
    }
    async fn create(
        &self,
        config: serde_json::Value,
    ) -> Result<Box<dyn Connector>, ConnectorError> {
        let config: crate::GithubConfig = serde_json::from_value(config)
            .map_err(|e| ConnectorError::Serialization(e.to_string()))?;
        let connector = crate::GithubConnector::new(config)
            .map_err(|e| ConnectorError::ExecutionFailed(e.to_string()))?;
        Ok(Box::new(connector))
    }
}

inventory::submit!(FactoryEntry {
    factory: &GithubFactory,
});
