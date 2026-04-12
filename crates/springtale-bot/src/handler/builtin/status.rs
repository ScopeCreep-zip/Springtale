//! `/status` — bot health.

use async_trait::async_trait;

use crate::error::BotError;
use crate::handler::registry::{Handler, HandlerContext, HandlerResult};

pub struct StatusHandler;

#[async_trait]
impl Handler for StatusHandler {
    async fn handle(&self, _args: &str, _ctx: &HandlerContext) -> Result<HandlerResult, BotError> {
        Ok(HandlerResult {
            response: "Bot is running.".into(),
        })
    }

    fn description(&self) -> &str {
        "Show bot status"
    }

    fn is_builtin(&self) -> bool {
        true
    }
}
