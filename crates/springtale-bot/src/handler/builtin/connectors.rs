//! `/connectors` — list installed connectors.

use async_trait::async_trait;

use crate::error::BotError;
use crate::handler::registry::{Handler, HandlerContext, HandlerResult};

pub struct ConnectorsHandler;

#[async_trait]
impl Handler for ConnectorsHandler {
    async fn handle(&self, _args: &str, ctx: &HandlerContext) -> Result<HandlerResult, BotError> {
        let registry = ctx.registry.read().await;
        let list = registry.list();
        if list.is_empty() {
            return Ok(HandlerResult {
                response: "No connectors installed.".into(),
            });
        }
        let mut lines = Vec::with_capacity(list.len() + 1);
        lines.push(format!("{} connectors:", list.len()));
        for (name, enabled) in &list {
            let status = if *enabled { "enabled" } else { "disabled" };
            lines.push(format!("  {} [{}]", name, status));
        }
        Ok(HandlerResult {
            response: lines.join("\n"),
        })
    }

    fn description(&self) -> &str {
        "List installed connectors"
    }

    fn is_builtin(&self) -> bool {
        true
    }
}
