//! `/help` — list available commands.

use async_trait::async_trait;

use super::BUILTIN_COMMANDS;
use crate::error::BotError;
use crate::handler::registry::{Handler, HandlerContext, HandlerResult};

pub struct HelpHandler;

#[async_trait]
impl Handler for HelpHandler {
    async fn handle(&self, _args: &str, _ctx: &HandlerContext) -> Result<HandlerResult, BotError> {
        let mut lines = vec!["Available commands:".to_owned()];
        for cmd in BUILTIN_COMMANDS {
            lines.push(format!("  /{cmd}"));
        }
        Ok(HandlerResult {
            response: lines.join("\n"),
        })
    }

    fn description(&self) -> &str {
        "List available commands"
    }

    fn is_builtin(&self) -> bool {
        true
    }
}
