//! `/toggle` — enable or disable a rule.

use async_trait::async_trait;

use crate::error::BotError;
use crate::handler::registry::{Handler, HandlerContext, HandlerResult};

pub struct ToggleHandler;

#[async_trait]
impl Handler for ToggleHandler {
    async fn handle(&self, args: &str, ctx: &HandlerContext) -> Result<HandlerResult, BotError> {
        let name = args.trim();
        if name.is_empty() {
            return Ok(HandlerResult {
                response: "Usage: /toggle <rule-name>".into(),
            });
        }

        let engine = ctx.engine.read().await;
        let rules = engine.list_rules();
        let rule = rules.iter().find(|r| r.name == name);
        match rule {
            Some(r) => {
                let new_status =
                    if r.status == springtale_core::rule::types::RuleStatus::Enabled {
                        springtale_core::rule::types::RuleStatus::Disabled
                    } else {
                        springtale_core::rule::types::RuleStatus::Enabled
                    };
                let id = r.id;
                let enabled = new_status == springtale_core::rule::types::RuleStatus::Enabled;
                let label = if enabled { "enabled" } else { "disabled" };
                drop(engine);

                let mut engine = ctx.engine.write().await;
                engine.set_status(&id, new_status);
                drop(engine);
                ctx.store.toggle_rule(&id, enabled).await?;

                Ok(HandlerResult {
                    response: format!("Rule '{name}' is now {label}."),
                })
            }
            None => Ok(HandlerResult {
                response: format!("Rule not found: {name}"),
            }),
        }
    }

    fn description(&self) -> &str {
        "Toggle a rule on/off"
    }

    fn is_builtin(&self) -> bool {
        true
    }
}
