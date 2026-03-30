use async_trait::async_trait;

use super::registry::{Handler, HandlerContext, HandlerResult};
use crate::error::BotError;

/// Generic handler that delegates to a connector action.
///
/// Created during auto-registration: one `ConnectorHandler` per action
/// from each installed connector.
pub struct ConnectorHandler {
    connector_name: String,
    action_name: String,
    desc: String,
}

impl ConnectorHandler {
    pub fn new(connector_name: String, action_name: String, description: String) -> Self {
        Self {
            connector_name,
            action_name,
            desc: description,
        }
    }
}

#[async_trait]
impl Handler for ConnectorHandler {
    async fn handle(&self, args: &str, ctx: &HandlerContext) -> Result<HandlerResult, BotError> {
        // Build input JSON from args. Simple key-value: pass args as "query" or "input".
        let input = if args.is_empty() {
            serde_json::json!({})
        } else {
            serde_json::json!({ "query": args, "input": args })
        };

        let registry = ctx.registry.read().await;
        let result = registry
            .execute(&self.connector_name, &self.action_name, input)
            .await?;

        if result.success {
            let output_str = if result.output.is_string() {
                result.output.as_str().unwrap_or(&result.message).to_owned()
            } else if result.output.is_null() {
                result.message
            } else {
                serde_json::to_string_pretty(&result.output)
                    .unwrap_or_else(|_| result.message.clone())
            };
            Ok(HandlerResult {
                response: output_str,
            })
        } else {
            Ok(HandlerResult {
                response: format!("Action failed: {}", result.message),
            })
        }
    }

    fn description(&self) -> &str {
        &self.desc
    }
}

/// Auto-register connector actions as bot commands.
///
/// For single-action connectors: action name = command name (e.g., `/search`).
/// For multi-action connectors: `/{connector_short} {action}` (e.g., `/github create_issue`).
///
/// Built-in commands are protected from override.
pub fn auto_register_connector_commands(
    handlers: &mut super::registry::HandlerRegistry,
    prefix_router: &mut crate::router::PrefixRouter,
    registry: &springtale_connector::registry::store::ConnectorRegistry,
) -> Result<(), BotError> {
    for (connector_name, enabled) in registry.list() {
        if !enabled {
            continue;
        }

        let Some(entry) = registry.get(connector_name) else {
            continue;
        };

        let actions = entry.host.actions();

        for action_decl in actions {
            let command_name = if actions.len() == 1 {
                action_decl.name.clone()
            } else {
                let short = connector_name
                    .strip_prefix("connector-")
                    .unwrap_or(connector_name);
                format!("{short} {}", action_decl.name)
            };

            // Skip if it would override a builtin
            if super::builtin::BUILTIN_COMMANDS.contains(&command_name.as_str()) {
                tracing::warn!(
                    command = %command_name,
                    connector = %connector_name,
                    "skipping auto-registration: would override builtin"
                );
                continue;
            }

            let handler = ConnectorHandler::new(
                connector_name.to_owned(),
                action_decl.name.clone(),
                action_decl.description.clone(),
            );

            if let Err(e) = handlers.register(command_name.clone(), Box::new(handler)) {
                tracing::warn!(
                    command = %command_name,
                    connector = %connector_name,
                    error = %e,
                    "failed to auto-register connector command"
                );
                continue;
            }

            prefix_router.register(&command_name);

            tracing::info!(
                command = %command_name,
                connector = %connector_name,
                action = %action_decl.name,
                "auto-registered connector command"
            );
        }
    }

    Ok(())
}
