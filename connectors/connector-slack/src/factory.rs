use springtale_connector::Connector;
use springtale_connector::error::ConnectorError;
use springtale_connector::factory::{ConnectorFactory, FactoryEntry, FormField, PlatformForm};
use springtale_connector::manifest::types::{ActionDecl, ConnectorManifest, TriggerDecl};

struct SlackFactory;

static SLACK_FORM: PlatformForm = PlatformForm {
    id: "slack",
    config_key: "slack",
    label: "Slack",
    description: "Connect a Slack app (Socket Mode)",
    setup_help: "Create an app at api.slack.com/apps, enable Socket Mode, generate a Bot token (xoxb-) and App token (xapp-).",
    fields: &[
        FormField {
            name: "bot_token",
            label: "Bot token",
            description: "xoxb-... bot user OAuth token",
            secret: true,
            default: None,
            required: true,
            validation: Some(r"^xoxb-"),
        },
        FormField {
            name: "app_token",
            label: "App token",
            description: "xapp-... app-level token (Socket Mode)",
            secret: true,
            default: None,
            required: true,
            validation: Some(r"^xapp-"),
        },
    ],
};

#[async_trait::async_trait]
impl ConnectorFactory for SlackFactory {
    fn name(&self) -> &'static str {
        "connector-slack"
    }
    fn config_key(&self) -> &'static str {
        "slack"
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
    fn onboarding_form(&self) -> Option<&'static PlatformForm> {
        Some(&SLACK_FORM)
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
