//! `/alias` — manage command aliases.

use async_trait::async_trait;

use crate::error::BotError;
use crate::handler::registry::{Handler, HandlerContext, HandlerResult};

pub struct AliasHandler;

#[async_trait]
impl Handler for AliasHandler {
    async fn handle(&self, args: &str, ctx: &HandlerContext) -> Result<HandlerResult, BotError> {
        let parts: Vec<&str> = args.trim().splitn(3, ' ').collect();
        match parts.first().copied() {
            Some("set") if parts.len() == 3 => {
                let alias = parts[1].trim();
                let target = parts[2].trim();
                if alias.is_empty() || target.is_empty() {
                    return Ok(HandlerResult {
                        response: "Usage: /alias set <alias> <command>".into(),
                    });
                }
                ctx.store.upsert_alias(alias, target, &ctx.user_id).await?;
                Ok(HandlerResult {
                    response: format!("Alias set: /{alias} → /{target}"),
                })
            }
            Some("remove") if parts.len() >= 2 => {
                let alias = parts[1];
                ctx.store.delete_alias(alias).await?;
                Ok(HandlerResult {
                    response: format!("Alias removed: {alias}"),
                })
            }
            _ => {
                let aliases = ctx.store.list_aliases().await?;
                if aliases.is_empty() {
                    return Ok(HandlerResult {
                        response: "No aliases defined. Use: /alias set <alias> <command>".into(),
                    });
                }
                let mut lines = vec!["Aliases:".to_owned()];
                for (alias, target) in &aliases {
                    lines.push(format!("  /{alias} → /{target}"));
                }
                Ok(HandlerResult {
                    response: lines.join("\n"),
                })
            }
        }
    }

    fn description(&self) -> &str {
        "Manage command aliases"
    }

    fn is_builtin(&self) -> bool {
        true
    }
}
