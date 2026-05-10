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
        let params = if args.is_empty() {
            serde_json::Map::new()
        } else {
            let mut m = serde_json::Map::new();
            m.insert("query".into(), serde_json::Value::String(args.into()));
            m.insert("input".into(), serde_json::Value::String(args.into()));
            m
        };

        // Route every chat-command-driven connector call through
        // `dispatch_action*` — same path the rule-trigger, daemon-queue,
        // and formation-tick paths use. That guarantees
        // `sentinel.evaluate` runs before every network call
        // (SECURITY.md §6.10) and, when a formation tier is bound on
        // the context, the tier flows through the bridge to select the
        // right per-tier WASM `InstancePre`.
        let action = springtale_core::rule::action::Action::RunConnector {
            connector: self.connector_name.clone(),
            action: self.action_name.clone(),
            params,
        };
        let dispatch_outcome = match ctx.formation_tier {
            Some(tier) => {
                springtale_runtime::dispatch::dispatch_action_with_tier(
                    &action,
                    &ctx.capability_bridge,
                    &ctx.sentinel,
                    tier,
                )
                .await
            }
            None => {
                springtale_runtime::dispatch::dispatch_action(
                    &action,
                    &ctx.capability_bridge,
                    &ctx.sentinel,
                )
                .await
            }
        };

        match dispatch_outcome {
            Ok(msg) => Ok(HandlerResult { response: msg }),
            Err(e) => Ok(HandlerResult {
                response: format!("Action failed: {e}"),
            }),
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
