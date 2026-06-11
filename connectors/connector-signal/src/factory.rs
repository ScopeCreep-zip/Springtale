use springtale_connector::Connector;
use springtale_connector::error::ConnectorError;
use springtale_connector::factory::{ConnectorFactory, FactoryEntry, FormField, PlatformForm};
use springtale_connector::manifest::types::{ActionDecl, TriggerDecl};

struct SignalFactory;

static SIGNAL_FORM: PlatformForm = PlatformForm {
    id: "signal",
    config_key: "signal",
    label: "Signal",
    description: "Connect via a signal-cli daemon",
    setup_help: "Install signal-cli and run it in daemon mode. See https://github.com/AsamK/signal-cli.",
    fields: &[
        FormField {
            name: "daemon_url",
            label: "Daemon URL",
            description: "Address where signal-cli is listening",
            secret: false,
            default: Some("http://localhost:8080"),
            required: true,
            validation: Some(r"^https?://"),
        },
        FormField {
            name: "account_id",
            label: "Account ID",
            description: "Phone number / account identifier registered with signal-cli",
            secret: false,
            default: Some("default"),
            required: true,
            validation: None,
        },
    ],
};

#[async_trait::async_trait]
impl ConnectorFactory for SignalFactory {
    fn name(&self) -> &'static str {
        "connector-signal"
    }
    fn config_key(&self) -> &'static str {
        "signal"
    }
    fn trigger_declarations(&self) -> Vec<TriggerDecl> {
        crate::triggers::trigger_declarations()
    }
    fn action_declarations(&self) -> Vec<ActionDecl> {
        crate::actions::action_declarations()
    }
    fn onboarding_form(&self) -> Option<&'static PlatformForm> {
        Some(&SIGNAL_FORM)
    }
    fn config_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "object",
            "properties": {
                "daemon_url": {
                    "type": "string",
                    "description": "signal-cli daemon HTTP endpoint (e.g. http://localhost:8080)"
                },
                "account_id": {
                    "type": "string",
                    "description": "Account identifier (user-chosen, NOT your phone number)"
                },
                "message_jitter_secs": {
                    "type": "integer",
                    "description": "Publish-side jitter in seconds",
                    "default": 0
                }
            },
            "required": ["daemon_url", "account_id"]
        }))
    }
    async fn create(
        &self,
        config: serde_json::Value,
    ) -> Result<Box<dyn Connector>, ConnectorError> {
        let config: crate::SignalConfig = serde_json::from_value(config)
            .map_err(|e| ConnectorError::Serialization(e.to_string()))?;
        let connector = crate::SignalConnector::new(&config)
            .map_err(|e| ConnectorError::ExecutionFailed(e.to_string()))?;
        Ok(Box::new(connector))
    }
}

inventory::submit!(FactoryEntry {
    factory: &SignalFactory,
});
