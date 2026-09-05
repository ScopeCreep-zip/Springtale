use springtale_connector::Connector;
use springtale_connector::error::ConnectorError;
use springtale_connector::factory::{ConnectorFactory, FactoryEntry};
use springtale_connector::manifest::types::{ActionDecl, ConnectorManifest, TriggerDecl};

struct KickFactory;

#[async_trait::async_trait]
impl ConnectorFactory for KickFactory {
    fn name(&self) -> &'static str {
        "connector-kick"
    }
    fn config_key(&self) -> &'static str {
        "kick"
    }
    fn manifest(&self) -> ConnectorManifest {
        crate::connector::build_manifest(
            &crate::triggers::trigger_declarations(),
            &crate::actions::action_declarations(),
        )
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
                "client_id": {
                    "type": "string",
                    "description": "Kick OAuth2 client ID"
                },
                "client_secret": {
                    "type": "string",
                    "description": "Kick OAuth2 client secret",
                    "x-secret": true
                },
                "access_token": {
                    "type": "string",
                    "description": "OAuth2 access token (obtained after auth flow)",
                    "x-secret": true
                },
                "redirect_uri": {
                    "type": "string",
                    "description": "OAuth2 redirect URI"
                },
                "scopes": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "OAuth2 scopes",
                    "default": ["user:read", "channel:read", "channel:write", "chat:write", "events:subscribe"]
                },
                "api_base": {
                    "type": "string",
                    "description": "Kick API base URL",
                    "default": "https://api.kick.com"
                },
                "oauth_base": {
                    "type": "string",
                    "description": "Kick OAuth base URL",
                    "default": "https://id.kick.com"
                },
                "webhook_callback_url": {
                    "type": "string",
                    "description": "Webhook callback URL for event delivery"
                }
            },
            "required": ["client_id", "client_secret", "redirect_uri", "access_token"]
        }))
    }
    async fn create(
        &self,
        config: serde_json::Value,
    ) -> Result<Box<dyn Connector>, ConnectorError> {
        // Kick requires an access_token alongside the config.
        // The token is provided in the same config section:
        //   [kick]
        //   access_token = "..."
        let access_token = config
            .get("access_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ConnectorError::Serialization(
                    "kick config requires 'access_token' field".to_owned(),
                )
            })?;
        let token = secrecy::SecretBox::new(Box::new(access_token.to_owned()));

        let config: crate::KickConfig = serde_json::from_value(config)
            .map_err(|e| ConnectorError::Serialization(e.to_string()))?;
        let connector = crate::KickConnector::new(&config, token)
            .map_err(|e| ConnectorError::ExecutionFailed(e.to_string()))?;
        Ok(Box::new(connector))
    }
}

inventory::submit!(FactoryEntry {
    factory: &KickFactory,
});
