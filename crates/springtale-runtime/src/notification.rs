//! Delivery layer — a fired `Notify` / `SendMessage` step must reach
//! the user.
//!
//! A scheduled rule fires its action chain in the embedded job
//! consumer with nothing watching: `Action::Notify` only wrote a
//! `tracing::info!` line and `Action::SendMessage` (which carries no
//! connector destination) logged "no destination context". So seven
//! shipped recipes — weather-briefing, hydration-reminder,
//! daily-shutdown-checklist, tor-circuit-rotate-reminder,
//! travel-mode-friday-prep (Notify), cron-runner, github-pr-watcher
//! (SendMessage) — fired correctly and the user received **nothing**.
//!
//! The fix is a pub/sub fan-out: the job consumer holds the finished
//! [`ChainContext`], walks it for user-facing delivery steps, and
//! broadcasts each as a [`NotificationEvent`] on
//! [`RuntimeState::notification_tx`](crate::state::RuntimeState).
//! Subscribers fan it out to delivery channels — the in-app chat
//! stream (daemon SSE / desktop Tauri event) and a best-effort OS
//! notification. This is the standard scheduled-job → topic →
//! channel notification architecture (AWS pub/sub fanout, Azure
//! Notification Hubs, Cloud Scheduler + Pub/Sub), and it fixes every
//! current and future `Notify`/`SendMessage` recipe at once.
//!
//! Connector sends (`RunConnector send_*` — Telegram, Signal, etc.)
//! are NOT routed here: those reach the user through the connector's
//! own delivery path. This layer covers only the in-app delivery
//! actions that otherwise vanish.

use serde::{Deserialize, Serialize};
use springtale_core::rule::{ChainContext, StepOutput};

/// A user-facing message produced by a fired rule chain, fanned out
/// to the in-app chat stream and OS notifications.
///
/// `Serialize` so the desktop Tauri `chat-message` event and the
/// daemon SSE frame can carry it across the IPC/HTTP boundary;
/// `Deserialize` for the desktop side to reconstruct it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationEvent {
    /// Short heading (the `Notify` title, or "Message" for a
    /// destination-less `SendMessage`).
    pub title: String,
    /// The human-readable body the recipe composed.
    pub body: String,
}

impl NotificationEvent {
    /// Walk a finished chain and emit one event per user-facing
    /// delivery step, in chain order. A recipe may deliver more than
    /// once (e.g. a digest that notifies per section), so this
    /// returns every match rather than just the last.
    pub fn from_chain(chain: &ChainContext) -> Vec<NotificationEvent> {
        chain.steps.iter().filter_map(Self::from_step).collect()
    }

    /// Project a single step into a delivery event, or `None` if the
    /// step isn't a user-facing delivery action.
    fn from_step(step: &StepOutput) -> Option<NotificationEvent> {
        let field = |key: &str| {
            step.output
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        };
        match step.kind.as_str() {
            "notify" => Some(NotificationEvent {
                title: field("title").unwrap_or_else(|| "Springtale".to_owned()),
                body: field("body").unwrap_or_default(),
            }),
            // `SendMessage` carries no connector destination — it's an
            // in-app message, so it routes through the chat stream.
            "send_message" => Some(NotificationEvent {
                title: "Message".to_owned(),
                body: field("text")?,
            }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use springtale_core::rule::StepOutput;

    fn step(kind: &str, output: serde_json::Value) -> StepOutput {
        StepOutput {
            index: 1,
            kind: kind.to_owned(),
            name: None,
            output,
            duration_ms: 0,
            error: None,
        }
    }

    #[test]
    fn test_from_chain_emits_notify_title_and_body() {
        let mut chain = ChainContext::new(serde_json::json!({}));
        chain.record_step(step(
            "notify",
            serde_json::json!({ "title": "Weather", "body": "It's 72°F in Sacramento." }),
        ));
        let events = NotificationEvent::from_chain(&chain);
        assert_eq!(
            events,
            vec![NotificationEvent {
                title: "Weather".to_owned(),
                body: "It's 72°F in Sacramento.".to_owned(),
            }]
        );
    }

    #[test]
    fn test_from_chain_maps_send_message_to_message_event() {
        let mut chain = ChainContext::new(serde_json::json!({}));
        chain.record_step(step(
            "send_message",
            serde_json::json!({ "text": "build passed" }),
        ));
        let events = NotificationEvent::from_chain(&chain);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].title, "Message");
        assert_eq!(events[0].body, "build passed");
    }

    #[test]
    fn test_from_chain_ignores_non_delivery_steps() {
        let mut chain = ChainContext::new(serde_json::json!({}));
        chain.record_step(step(
            "run_connector",
            serde_json::json!({ "body": "{...}" }),
        ));
        chain.record_step(step("extract", serde_json::json!({ "temp": "72" })));
        assert!(NotificationEvent::from_chain(&chain).is_empty());
    }

    #[test]
    fn test_from_chain_notify_defaults_title_when_absent() {
        let mut chain = ChainContext::new(serde_json::json!({}));
        chain.record_step(step("notify", serde_json::json!({ "body": "hi" })));
        let events = NotificationEvent::from_chain(&chain);
        assert_eq!(events[0].title, "Springtale");
        assert_eq!(events[0].body, "hi");
    }
}
