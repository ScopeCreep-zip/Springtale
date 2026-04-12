//! `/disable` — disable a connector.

use async_trait::async_trait;

use crate::error::BotError;
use crate::handler::registry::{Handler, HandlerContext, HandlerResult};

pub struct DisableHandler;

#[async_trait]
impl Handler for DisableHandler {
    async fn handle(&self, args: &str, ctx: &HandlerContext) -> Result<HandlerResult, BotError> {
        let name = args.trim();
        if name.is_empty() {
            return Ok(HandlerResult {
                response: "Usage: /disable <connector-name>".into(),
            });
        }

        let mut registry = ctx.registry.write().await;
        if registry.disable(name).is_err() {
            return Ok(HandlerResult {
                response: format!("Connector not found: {name}"),
            });
        }
        drop(registry);
        ctx.store.set_connector_enabled(name, false).await?;

        Ok(HandlerResult {
            response: format!("Connector '{name}' disabled."),
        })
    }

    fn description(&self) -> &str {
        "Disable a connector"
    }

    fn is_builtin(&self) -> bool {
        true
    }
}
