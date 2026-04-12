//! `/delrule` — delete a rule by name.

use async_trait::async_trait;

use crate::error::BotError;
use crate::handler::registry::{Handler, HandlerContext, HandlerResult};

pub struct DelRuleHandler;

#[async_trait]
impl Handler for DelRuleHandler {
    async fn handle(&self, args: &str, ctx: &HandlerContext) -> Result<HandlerResult, BotError> {
        let name = args.trim();
        if name.is_empty() {
            return Ok(HandlerResult {
                response: "Usage: /delrule <rule-name>".into(),
            });
        }

        let engine = ctx.engine.read().await;
        let rules = engine.list_rules();
        let Some(rule) = rules.iter().find(|r| r.name == name) else {
            return Ok(HandlerResult {
                response: format!("Rule not found: {name}"),
            });
        };
        let id = rule.id;
        drop(engine);

        let mut engine = ctx.engine.write().await;
        engine.remove_rule(&id);
        drop(engine);

        if let Err(e) = ctx.store.delete_rule(&id).await {
            return Ok(HandlerResult {
                response: format!("Engine removed but store delete failed: {e}"),
            });
        }

        Ok(HandlerResult {
            response: format!("Rule '{name}' deleted."),
        })
    }

    fn description(&self) -> &str {
        "Delete a rule by name"
    }

    fn is_builtin(&self) -> bool {
        true
    }
}
