//! `/pair` — DM pairing (approve, revoke, list). Owner-only.

use async_trait::async_trait;

use crate::error::BotError;
use crate::handler::registry::{Handler, HandlerContext, HandlerResult};

pub struct PairHandler;

#[async_trait]
impl Handler for PairHandler {
    async fn handle(&self, args: &str, ctx: &HandlerContext) -> Result<HandlerResult, BotError> {
        if !caller_is_owner(ctx).await? {
            return Ok(HandlerResult {
                response: "Only the bot owner can manage pairing.".into(),
            });
        }

        let parts: Vec<&str> = args.trim().splitn(3, ' ').collect();
        match parts.first().copied() {
            Some("approve") if parts.len() >= 2 => approve(parts[1].trim(), ctx).await,
            Some("revoke") if parts.len() >= 2 => revoke(parts[1].trim(), ctx).await,
            Some("list") | None => list(ctx).await,
            _ => Ok(HandlerResult {
                response: "Usage: /pair approve <code> | /pair revoke <user_id> | /pair list"
                    .into(),
            }),
        }
    }

    fn description(&self) -> &str {
        "Manage DM pairing (approve/revoke users)"
    }

    fn is_builtin(&self) -> bool {
        true
    }
}

const PAIRING_CODE_TTL_MINUTES: i64 = 60;

async fn caller_is_owner(ctx: &HandlerContext) -> Result<bool, BotError> {
    let owner_raw = ctx
        .store
        .get_config("bot:owner_id")
        .await?
        .unwrap_or_default();
    let owner = owner_raw.trim_matches('"');
    Ok(owner == ctx.user_id)
}

async fn approve(code: &str, ctx: &HandlerContext) -> Result<HandlerResult, BotError> {
    let code_key = format!("pairing_code:{code}");
    let Some(val) = ctx.store.get_config(&code_key).await? else {
        return Ok(HandlerResult {
            response: format!("Unknown pairing code: {code}"),
        });
    };

    let data: serde_json::Value = serde_json::from_str(&val)
        .map_err(|e| BotError::Session(format!("invalid pairing data: {e}")))?;

    if let Some(created) = data["created_at"].as_str()
        && let Ok(created_time) = chrono::DateTime::parse_from_rfc3339(created)
    {
        let elapsed = chrono::Utc::now() - created_time.with_timezone(&chrono::Utc);
        if elapsed.num_minutes() > PAIRING_CODE_TTL_MINUTES {
            let _ = ctx.store.delete_config(&code_key).await;
            return Ok(HandlerResult {
                response: format!(
                    "Pairing code {code} has expired. Ask the user to send a new \
                     message to get a fresh code."
                ),
            });
        }
    }

    let user_id = data["user_id"]
        .as_str()
        .ok_or_else(|| BotError::Session("missing user_id".into()))?;

    let paired_key = format!("paired:{user_id}");
    let paired_val = serde_json::json!({
        "approved_at": chrono::Utc::now().to_rfc3339(),
        "approved_by": ctx.user_id,
    })
    .to_string();
    ctx.store.set_config(&paired_key, &paired_val).await?;

    let _ = ctx.store.delete_config(&code_key).await;

    Ok(HandlerResult {
        response: format!("User {user_id} paired successfully."),
    })
}

async fn revoke(user_id: &str, ctx: &HandlerContext) -> Result<HandlerResult, BotError> {
    let paired_key = format!("paired:{user_id}");
    ctx.store.delete_config(&paired_key).await?;
    Ok(HandlerResult {
        response: format!("User {user_id} access revoked."),
    })
}

async fn list(ctx: &HandlerContext) -> Result<HandlerResult, BotError> {
    let all = ctx.store.list_config().await?;
    let paired: Vec<_> = all
        .iter()
        .filter(|(k, _)| k.starts_with("paired:"))
        .map(|(k, _)| k.strip_prefix("paired:").unwrap_or(k))
        .collect();
    if paired.is_empty() {
        return Ok(HandlerResult {
            response: "No paired users.".into(),
        });
    }
    let mut lines = vec![format!("{} paired users:", paired.len())];
    for user in &paired {
        lines.push(format!("  {user}"));
    }
    Ok(HandlerResult {
        response: lines.join("\n"),
    })
}
