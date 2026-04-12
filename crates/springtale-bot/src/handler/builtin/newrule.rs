//! `/newrule` — create a rule from a TOML snippet.
//!
//! Takes a TOML body as the command argument, parses it into a `Rule`,
//! and inserts into both the store and the engine. Mirrors the POST
//! `/rules` API handler but as a chat command so a bot owner can add
//! rules from their phone without touching a dashboard.

use async_trait::async_trait;

use springtale_core::rule::types::Rule;

use crate::error::BotError;
use crate::handler::registry::{Handler, HandlerContext, HandlerResult};

pub struct NewRuleHandler;

#[async_trait]
impl Handler for NewRuleHandler {
    async fn handle(&self, args: &str, ctx: &HandlerContext) -> Result<HandlerResult, BotError> {
        let body = args.trim();
        if body.is_empty() {
            return Ok(HandlerResult {
                response: "Usage: /newrule <toml>\n\n\
                           Example:\n\
                           /newrule [rule]\\nname=\"greet\"\\n[trigger]\\ntype=\"ConnectorEvent\"\\nconnector=\"connector-telegram\"\\nevent=\"command_received\""
                    .into(),
            });
        }

        let rule: Rule = match toml::from_str(body) {
            Ok(r) => r,
            Err(e) => {
                return Ok(HandlerResult {
                    response: format!("Invalid rule TOML: {e}"),
                });
            }
        };

        // Insert into store first so a failing engine add can roll back.
        if let Err(e) = ctx.store.insert_rule(&rule).await {
            return Ok(HandlerResult {
                response: format!("Failed to persist rule: {e}"),
            });
        }

        let name = rule.name.clone();
        let id = rule.id;
        let mut engine = ctx.engine.write().await;
        if let Err(e) = engine.add_rule(rule) {
            // Roll back the store insert.
            drop(engine);
            let _ = ctx.store.delete_rule(&id).await;
            return Ok(HandlerResult {
                response: format!("Rule rejected by engine: {e}"),
            });
        }

        Ok(HandlerResult {
            response: format!("Rule '{name}' created and enabled."),
        })
    }

    fn description(&self) -> &str {
        "Create a rule from a TOML snippet"
    }

    fn is_builtin(&self) -> bool {
        true
    }
}
