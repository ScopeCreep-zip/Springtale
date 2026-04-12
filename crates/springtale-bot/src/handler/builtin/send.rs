//! `/send` — cross-channel messaging.
//!
//! Thin wrapper around `springtale_runtime::operations::cross_channel`.
//! All validation and payload normalization lives in the runtime
//! operation so future callers (HTTP API, AI tool-call) get the same
//! behaviour.

use async_trait::async_trait;

use springtale_runtime::operations::cross_channel::{self, SendRequest};

use crate::error::BotError;
use crate::handler::registry::{Handler, HandlerContext, HandlerResult};

pub struct SendHandler;

#[async_trait]
impl Handler for SendHandler {
    async fn handle(&self, args: &str, ctx: &HandlerContext) -> Result<HandlerResult, BotError> {
        let parts: Vec<&str> = args.trim().splitn(3, ' ').collect();
        if parts.len() < 3 {
            return Ok(HandlerResult {
                response: "Usage: /send <connector> <channel_id> <message>\n\
                           Example: /send connector-discord 123456789 Hello from Telegram!"
                    .into(),
            });
        }

        let req = SendRequest {
            connector: parts[0].to_owned(),
            channel_id: parts[1].to_owned(),
            text: parts[2].to_owned(),
        };

        match cross_channel::send_via_registry(&ctx.registry, req).await {
            Ok(outcome) => Ok(HandlerResult {
                response: format!("Sent to {}: {}", outcome.connector, outcome.message),
            }),
            Err(e) => Ok(HandlerResult {
                response: format!("Failed: {e}"),
            }),
        }
    }

    fn description(&self) -> &str {
        "Send a message to another platform"
    }

    fn is_builtin(&self) -> bool {
        true
    }
}
