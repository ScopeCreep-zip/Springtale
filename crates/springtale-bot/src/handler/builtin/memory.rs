//! `/memory` — audit and compact what the bot remembers (plan 5.4).

use async_trait::async_trait;
use springtale_runtime::operations::memory;

use crate::error::BotError;
use crate::handler::registry::{Handler, HandlerContext, HandlerResult};

pub struct MemoryHandler;

/// Rows kept per session by `/memory compact` when no limit is given.
const DEFAULT_KEEP: usize = 50;

const USAGE: &str = "Usage: /memory [audit|compact [rows-to-keep]]";

#[async_trait]
impl Handler for MemoryHandler {
    async fn handle(&self, args: &str, ctx: &HandlerContext) -> Result<HandlerResult, BotError> {
        let parts: Vec<&str> = args.split_whitespace().collect();
        let response = match parts.as_slice() {
            [] | ["audit"] => {
                let audit = memory::audit_memory(&*ctx.store)
                    .await
                    .map_err(|e| BotError::Handler(e.to_string()))?;
                format!(
                    "{} — {} session(s) on record.",
                    audit.total_memory_note,
                    audit.sessions.len()
                )
            }
            ["compact"] | ["compact", _] => {
                let keep = parts
                    .get(1)
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(DEFAULT_KEEP);
                let deleted = memory::compact_memory(&*ctx.store, keep)
                    .await
                    .map_err(|e| BotError::Handler(e.to_string()))?;
                format!("Compacted to {keep} rows per session — {deleted} entries removed.")
            }
            _ => USAGE.to_owned(),
        };
        Ok(HandlerResult { response })
    }

    fn description(&self) -> &str {
        "Audit or compact stored memory"
    }

    fn is_builtin(&self) -> bool {
        true
    }
}
