//! `/rules` — list rules from the engine.

use async_trait::async_trait;

use crate::error::BotError;
use crate::handler::registry::{Handler, HandlerContext, HandlerResult};

pub struct RulesHandler;

#[async_trait]
impl Handler for RulesHandler {
    async fn handle(&self, _args: &str, ctx: &HandlerContext) -> Result<HandlerResult, BotError> {
        let engine = ctx.engine.read().await;
        let rules = engine.list_rules();
        if rules.is_empty() {
            return Ok(HandlerResult {
                response: "No rules configured.".into(),
            });
        }
        let mut lines = Vec::with_capacity(rules.len() + 1);
        lines.push(format!("{} active rules:", rules.len()));
        for rule in rules {
            let status = if rule.status == springtale_core::rule::types::RuleStatus::Enabled {
                "enabled"
            } else {
                "disabled"
            };
            lines.push(format!("  {} [{}]", rule.name, status));
        }
        Ok(HandlerResult {
            response: lines.join("\n"),
        })
    }

    fn description(&self) -> &str {
        "List active rules"
    }

    fn is_builtin(&self) -> bool {
        true
    }
}
