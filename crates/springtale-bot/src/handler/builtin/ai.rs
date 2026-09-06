//! `/ai` — read and change the model configuration (plan 5.4).
//!
//! Colony-level only. Per-formation and per-agent adapters stay in the
//! settings surfaces that scope them (see the product model): chat sets
//! the default every bot inherits.

use async_trait::async_trait;
use springtale_runtime::operations::config::{AiTarget, configure_ai_adapter, get_config};

use crate::error::BotError;
use crate::handler::registry::{Handler, HandlerContext, HandlerResult, runtime_or_err};

pub struct AiHandler;

const USAGE: &str = "Usage: /ai get | /ai set <none|ollama|openai|anthropic>";

#[async_trait]
impl Handler for AiHandler {
    async fn handle(&self, args: &str, ctx: &HandlerContext) -> Result<HandlerResult, BotError> {
        let rt = runtime_or_err(ctx)?;
        let parts: Vec<&str> = args.split_whitespace().collect();
        let response = match parts.as_slice() {
            [] | ["get"] => {
                let cfg = get_config(&*ctx.store, &AiTarget::Colony.key())
                    .await
                    .map_err(|e| BotError::Handler(e.to_string()))?;
                let adapter = cfg
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("none (NoopAdapter)");
                let model = cfg.get("model").and_then(|v| v.as_str()).unwrap_or("—");
                format!("AI adapter: {adapter} · model: {model}")
            }
            ["set", adapter] => {
                let adapter = match *adapter {
                    "none" | "noop" => "noop",
                    a @ ("ollama" | "openai" | "anthropic") => a,
                    other => {
                        return Ok(HandlerResult {
                            response: format!("'{other}' is not an adapter.\n{USAGE}"),
                        });
                    }
                };
                // Keep whatever else is configured (model, host, key
                // reference) and change only the adapter type.
                let mut cfg = get_config(&*ctx.store, &AiTarget::Colony.key())
                    .await
                    .map_err(|e| BotError::Handler(e.to_string()))?;
                if !cfg.is_object() {
                    cfg = serde_json::json!({});
                }
                if let Some(map) = cfg.as_object_mut() {
                    map.insert("type".to_owned(), serde_json::json!(adapter));
                }
                configure_ai_adapter(rt, AiTarget::Colony, cfg)
                    .await
                    .map_err(|e| BotError::Handler(e.to_string()))?;
                format!("AI adapter is now {adapter}.")
            }
            _ => USAGE.to_owned(),
        };
        Ok(HandlerResult { response })
    }

    fn description(&self) -> &str {
        "Read or change the AI adapter"
    }

    fn is_builtin(&self) -> bool {
        true
    }
}
