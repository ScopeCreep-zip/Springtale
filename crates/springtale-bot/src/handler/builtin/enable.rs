//! `/enable` — enable a connector.

use async_trait::async_trait;

use crate::error::BotError;
use crate::handler::registry::{Handler, HandlerContext, HandlerResult};

pub struct EnableHandler;

#[async_trait]
impl Handler for EnableHandler {
    async fn handle(&self, args: &str, ctx: &HandlerContext) -> Result<HandlerResult, BotError> {
        let name = args.trim();
        if name.is_empty() {
            return Ok(HandlerResult {
                response: "Usage: /enable <connector-name>".into(),
            });
        }

        let mut registry = ctx.registry.write().await;
        if registry.enable(name).is_err() {
            return Ok(HandlerResult {
                response: format!("Connector not found: {name}"),
            });
        }
        drop(registry);
        ctx.store.set_connector_enabled(name, true).await?;

        Ok(HandlerResult {
            response: format!("Connector '{name}' enabled."),
        })
    }

    fn description(&self) -> &str {
        "Enable a connector"
    }

    fn is_builtin(&self) -> bool {
        true
    }
}
