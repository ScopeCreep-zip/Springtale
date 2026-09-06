//! `/formation` — steer a formation from chat (plan 5.4).
//!
//! The four orchestration groups plus inspection, and nothing else:
//! there is deliberately no `assign` sub-command. You steer a
//! formation; you never hand work to a named member (the drum rule).
//! Every branch goes through an existing runtime operation.

use async_trait::async_trait;
use springtale_runtime::operations::formations as f;

use super::resolve::resolve_formation;
use crate::error::BotError;
use crate::handler::registry::{Handler, HandlerContext, HandlerResult, runtime_or_err};

pub struct FormationHandler;

const USAGE: &str = "Usage: /formation [list|get|deploy|pause|resume|dissolve|rally|intent|guard|add|rm] <name> [value]";

#[async_trait]
impl Handler for FormationHandler {
    async fn handle(&self, args: &str, ctx: &HandlerContext) -> Result<HandlerResult, BotError> {
        let rt = runtime_or_err(ctx)?;
        let parts: Vec<&str> = args.split_whitespace().collect();
        let response = match parts.as_slice() {
            [] | ["list"] => {
                let list = f::list_formations(rt)
                    .await
                    .map_err(|e| BotError::Handler(e.to_string()))?;
                if list.is_empty() {
                    "No formations yet.".to_owned()
                } else {
                    let rows: Vec<String> = list
                        .iter()
                        .map(|x| {
                            format!(
                                "• {} — {} · {} · {} member(s) · {}",
                                x.name, x.status, x.intent, x.member_count, x.momentum_label
                            )
                        })
                        .collect();
                    rows.join("\n")
                }
            }
            ["get", rest @ ..] => {
                let (id, name) = resolve_formation(rt, &rest.join(" ")).await?;
                let d = f::get_formation(rt, &id)
                    .await
                    .map_err(|e| BotError::Handler(e.to_string()))?;
                format!(
                    "{name} — {} · intent {} · momentum {} · members: {}",
                    d.info.status,
                    d.info.intent,
                    d.info.momentum_label,
                    if d.info.members.is_empty() {
                        "none".to_owned()
                    } else {
                        d.info.members.join(", ")
                    }
                )
            }
            ["deploy", rest @ ..] => {
                let (id, name) = resolve_formation(rt, &rest.join(" ")).await?;
                f::deploy_formation(rt, &id)
                    .await
                    .map_err(|e| BotError::Handler(e.to_string()))?;
                format!("{name} deployed.")
            }
            ["pause", rest @ ..] => {
                let (id, name) = resolve_formation(rt, &rest.join(" ")).await?;
                f::pause_formation(rt, &id)
                    .await
                    .map_err(|e| BotError::Handler(e.to_string()))?;
                format!("{name} paused.")
            }
            ["resume", rest @ ..] => {
                let (id, name) = resolve_formation(rt, &rest.join(" ")).await?;
                f::resume_formation(rt, &id)
                    .await
                    .map_err(|e| BotError::Handler(e.to_string()))?;
                format!("{name} resumed.")
            }
            ["dissolve", rest @ ..] => {
                let (id, name) = resolve_formation(rt, &rest.join(" ")).await?;
                f::dissolve_formation(rt, &id)
                    .await
                    .map_err(|e| BotError::Handler(e.to_string()))?;
                format!("{name} dissolved.")
            }
            ["rally", rest @ ..] => {
                let (id, name) = resolve_formation(rt, &rest.join(" ")).await?;
                f::rally_formation(rt, &id)
                    .await
                    .map_err(|e| BotError::Handler(e.to_string()))?;
                format!("Rally sent to {name}.")
            }
            ["intent", name] => {
                let (id, name) = resolve_formation(rt, name).await?;
                let next = f::cycle_intent(rt, &id)
                    .await
                    .map_err(|e| BotError::Handler(e.to_string()))?;
                format!("{name}'s intent is now {next}.")
            }
            ["intent", name, value] => {
                let (id, name) = resolve_formation(rt, name).await?;
                f::update_intent(rt, &id, value)
                    .await
                    .map_err(|e| BotError::Handler(e.to_string()))?;
                format!("{name}'s intent set to {value}.")
            }
            ["guard", rest @ ..] => {
                let (id, name) = resolve_formation(rt, &rest.join(" ")).await?;
                let on = springtale_runtime::operations::config::toggle_formation_guard(rt, &id)
                    .await
                    .map_err(|e| BotError::Handler(e.to_string()))?;
                format!("{name}'s guard is {}.", if on { "on" } else { "off" })
            }
            ["add", name, connector] => {
                let (id, name) = resolve_formation(rt, name).await?;
                f::add_member(rt, &id, connector)
                    .await
                    .map_err(|e| BotError::Handler(e.to_string()))?;
                format!("Added {connector} to {name}.")
            }
            ["rm", name, connector] => {
                let (id, name) = resolve_formation(rt, name).await?;
                f::remove_member(rt, &id, connector)
                    .await
                    .map_err(|e| BotError::Handler(e.to_string()))?;
                format!("Removed {connector} from {name}.")
            }
            _ => USAGE.to_owned(),
        };
        Ok(HandlerResult { response })
    }

    fn description(&self) -> &str {
        "Steer a formation — list, get, deploy, pause, resume, dissolve, rally, intent, guard, add, rm"
    }

    fn is_builtin(&self) -> bool {
        true
    }
}
