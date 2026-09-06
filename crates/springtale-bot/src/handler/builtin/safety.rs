//! `/safety` — read and change the safety configuration (plan 5.4).
//!
//! A safety setting is the one thing a coerced user must not change by
//! accident, so a write needs an explicit `--confirm` argument. Reads
//! never do.

use async_trait::async_trait;
use springtale_runtime::operations::safety;

use crate::error::BotError;
use crate::handler::registry::{Handler, HandlerContext, HandlerResult, runtime_or_err};

pub struct SafetyHandler;

const USAGE: &str = "Usage: /safety get | /safety set <window-title|auto-lock-minutes|content-protected|panic-taps> <value> --confirm";

#[async_trait]
impl Handler for SafetyHandler {
    async fn handle(&self, args: &str, ctx: &HandlerContext) -> Result<HandlerResult, BotError> {
        let rt = runtime_or_err(ctx)?;
        let parts: Vec<&str> = args.split_whitespace().collect();
        let response = match parts.as_slice() {
            [] | ["get"] => {
                let cfg = safety::get_safety_config(rt)
                    .await
                    .map_err(|e| BotError::Handler(e.to_string()))?;
                format!(
                    "window-title: {} · auto-lock-minutes: {} · content-protected: {} · panic-taps: {} · disguise: {}",
                    cfg.window_title,
                    cfg.auto_lock_minutes,
                    cfg.content_protected,
                    cfg.panic_tap_count,
                    if cfg.disguise_active { "on" } else { "off" }
                )
            }
            ["set", key, value, "--confirm"] => {
                let mut cfg = safety::get_safety_config(rt)
                    .await
                    .map_err(|e| BotError::Handler(e.to_string()))?;
                match *key {
                    "window-title" => cfg.window_title = (*value).to_owned(),
                    "auto-lock-minutes" => {
                        cfg.auto_lock_minutes = value
                            .parse()
                            .map_err(|_| BotError::Handler("minutes must be a number".to_owned()))?
                    }
                    "content-protected" => {
                        cfg.content_protected = matches!(*value, "true" | "on" | "yes")
                    }
                    "panic-taps" => {
                        cfg.panic_tap_count = value
                            .parse()
                            .map_err(|_| BotError::Handler("taps must be a number".to_owned()))?
                    }
                    other => {
                        return Ok(HandlerResult {
                            response: format!("'{other}' is not a safety setting.\n{USAGE}"),
                        });
                    }
                }
                safety::save_safety_config(rt, cfg)
                    .await
                    .map_err(|e| BotError::Handler(e.to_string()))?;
                format!("Safety setting {key} is now {value}.")
            }
            ["set", ..] => "Safety changes need an explicit --confirm at the end.".to_owned(),
            _ => USAGE.to_owned(),
        };
        Ok(HandlerResult { response })
    }

    fn description(&self) -> &str {
        "Read or change the safety configuration"
    }

    fn is_builtin(&self) -> bool {
        true
    }
}
