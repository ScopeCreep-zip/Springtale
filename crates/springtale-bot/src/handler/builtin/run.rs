//! `/run` — dry-run a rule.
//!
//! Looks up a rule by name and evaluates it against a synthetic trigger
//! via `springtale_runtime::operations::rules::execute::run_rule_standalone`.
//! No side effects — actions are counted but not executed — so this is
//! safe to expose to bot owners for troubleshooting.

use async_trait::async_trait;

use springtale_runtime::operations::rules::run_rule_standalone;

use crate::error::BotError;
use crate::handler::registry::{Handler, HandlerContext, HandlerResult};

pub struct RunHandler;

#[async_trait]
impl Handler for RunHandler {
    async fn handle(&self, args: &str, ctx: &HandlerContext) -> Result<HandlerResult, BotError> {
        let name = args.trim();
        if name.is_empty() {
            return Ok(HandlerResult {
                response: "Usage: /run <rule-name>".into(),
            });
        }

        let engine = ctx.engine.read().await;
        let rules = engine.list_rules();
        let Some(rule) = rules.iter().find(|r| r.name == name) else {
            return Ok(HandlerResult {
                response: format!("Rule not found: {name}"),
            });
        };
        let rule = (*rule).clone();
        drop(engine);

        let result = run_rule_standalone(&rule);
        Ok(HandlerResult {
            response: format!(
                "Rule '{name}' dry-run: matched={} actions={}",
                result.matched, result.actions_count,
            ),
        })
    }

    fn description(&self) -> &str {
        "Dry-run a rule (no side effects)"
    }

    fn is_builtin(&self) -> bool {
        true
    }
}
