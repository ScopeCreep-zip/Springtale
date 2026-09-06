//! `/approvals` — see and answer the approval queue from chat (plan 5.4).
//!
//! The same gate the inline approval card resolves
//! (`capability_bridge.approval_gate()`), so a request answered by name
//! here and one answered by tapping the card land identically.

use async_trait::async_trait;

use crate::error::BotError;
use crate::handler::registry::{Handler, HandlerContext, HandlerResult};

pub struct ApprovalsHandler;

const USAGE: &str = "Usage: /approvals [list|approve <id>|deny <id>]";

#[async_trait]
impl Handler for ApprovalsHandler {
    async fn handle(&self, args: &str, ctx: &HandlerContext) -> Result<HandlerResult, BotError> {
        let Some(gate) = ctx.capability_bridge.approval_gate() else {
            return Ok(HandlerResult {
                response: "No approval gate is wired on this instance.".to_owned(),
            });
        };
        let parts: Vec<&str> = args.split_whitespace().collect();
        let response = match parts.as_slice() {
            [] | ["list"] => {
                let pending = gate.pending().await;
                if pending.is_empty() {
                    "Nothing is waiting for approval.".to_owned()
                } else {
                    let rows: Vec<String> = pending
                        .iter()
                        .map(|r| {
                            format!("• {} — {} wants {:?}", r.id, r.connector_name, r.capability)
                        })
                        .collect();
                    rows.join("\n")
                }
            }
            ["approve", id] | ["deny", id] => {
                let approved = parts[0] == "approve";
                let uuid = uuid::Uuid::parse_str(id)
                    .map_err(|_| BotError::Handler(format!("'{id}' is not an approval id")))?;
                let decision = if approved {
                    springtale_runtime::approval::ApprovalDecision::Approved {
                        approver: format!("owner ({})", ctx.user_id),
                        approved_at: chrono::Utc::now(),
                    }
                } else {
                    springtale_runtime::approval::ApprovalDecision::Denied {
                        reason: "denied from chat".to_owned(),
                        denied_at: chrono::Utc::now(),
                    }
                };
                match gate
                    .resolve(
                        springtale_runtime::approval::ApprovalRequestId(uuid),
                        decision,
                    )
                    .await
                {
                    Ok(()) if approved => "Approved — running it now.".to_owned(),
                    Ok(()) => "Denied — nothing was run.".to_owned(),
                    Err(_) => "That approval already closed (or expired).".to_owned(),
                }
            }
            _ => USAGE.to_owned(),
        };
        Ok(HandlerResult { response })
    }

    fn description(&self) -> &str {
        "See and answer the approval queue"
    }

    fn is_builtin(&self) -> bool {
        true
    }
}
