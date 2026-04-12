//! `/events` — show recent events.

use async_trait::async_trait;

use crate::error::BotError;
use crate::handler::registry::{Handler, HandlerContext, HandlerResult};

const EVENTS_LIMIT: usize = 10;

pub struct EventsHandler;

#[async_trait]
impl Handler for EventsHandler {
    async fn handle(&self, _args: &str, ctx: &HandlerContext) -> Result<HandlerResult, BotError> {
        let filter = springtale_store::schema::events::EventFilter::default();
        let events = ctx.store.list_events(&filter).await?;

        if events.is_empty() {
            return Ok(HandlerResult {
                response: "No events recorded.".into(),
            });
        }

        let show = &events[..events.len().min(EVENTS_LIMIT)];
        let mut lines = vec![format!("Last {} events:", show.len())];
        for event in show {
            lines.push(format!(
                "  [{}] {} {} → {}",
                event.timestamp.format("%H:%M:%S"),
                event.connector_name,
                event.trigger_type,
                if event.action_taken.is_empty() {
                    "—"
                } else {
                    &event.action_taken
                },
            ));
        }
        Ok(HandlerResult {
            response: lines.join("\n"),
        })
    }

    fn description(&self) -> &str {
        "Show recent events"
    }

    fn is_builtin(&self) -> bool {
        true
    }
}
