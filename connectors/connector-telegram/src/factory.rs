use springtale_connector::Connector;
use springtale_connector::error::ConnectorError;
use springtale_connector::factory::{ConnectorFactory, FactoryEntry, FormField, PlatformForm};
use springtale_connector::manifest::types::{ActionDecl, TriggerDecl};

struct TelegramFactory;

static TELEGRAM_FORM: PlatformForm = PlatformForm {
    id: "telegram",
    config_key: "telegram",
    label: "Telegram",
    description: "Connect a Telegram bot via polling",
    setup_help: "Create a bot with @BotFather in Telegram. Copy the HTTP API token it returns.",
    fields: &[
        FormField {
            name: "bot_token",
            label: "Bot token",
            description: "Telegram Bot API token from @BotFather",
            secret: true,
            default: None,
            required: true,
            validation: Some(r"^\d+:[A-Za-z0-9_-]+$"),
        },
        FormField {
            name: "update_mode",
            label: "Update mode",
            description: "polling (no public URL needed) or webhook",
            secret: false,
            default: Some("polling"),
            required: false,
            validation: Some(r"^(polling|webhook)$"),
        },
    ],
};

#[async_trait::async_trait]
impl ConnectorFactory for TelegramFactory {
    fn name(&self) -> &'static str {
        "connector-telegram"
    }
    fn config_key(&self) -> &'static str {
        "telegram"
    }
    fn trigger_declarations(&self) -> Vec<TriggerDecl> {
        crate::triggers::trigger_declarations()
    }
    fn action_declarations(&self) -> Vec<ActionDecl> {
        crate::actions::action_declarations()
    }
    fn onboarding_form(&self) -> Option<&'static PlatformForm> {
        Some(&TELEGRAM_FORM)
    }
    fn config_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "object",
            "properties": {
                "bot_token": {
                    "type": "string",
                    "description": "Bot token from @BotFather (format: 123456:ABC-DEF...)",
                    "x-secret": true
                },
                "api_base": {
                    "type": "string",
                    "description": "Telegram Bot API base URL",
                    "default": "https://api.telegram.org"
                },
                "update_mode": {
                    "type": "string",
                    "description": "Update mode: polling or webhook",
                    "default": "polling",
                    "enum": ["polling", "webhook"]
                },
                "webhook_url": {
                    "type": "string",
                    "description": "Webhook callback URL (required when update_mode = webhook)"
                },
                "poll_timeout": {
                    "type": "integer",
                    "description": "Long-polling timeout in seconds",
                    "default": 30
                }
            },
            "required": ["bot_token"]
        }))
    }
    async fn create(
        &self,
        config: serde_json::Value,
    ) -> Result<Box<dyn Connector>, ConnectorError> {
        let config: crate::TelegramConfig = serde_json::from_value(config)
            .map_err(|e| ConnectorError::Serialization(e.to_string()))?;
        let connector = crate::TelegramConnector::new(&config)
            .map_err(|e| ConnectorError::ExecutionFailed(e.to_string()))?;
        Ok(Box::new(connector))
    }
}

inventory::submit!(FactoryEntry {
    factory: &TelegramFactory,
});
