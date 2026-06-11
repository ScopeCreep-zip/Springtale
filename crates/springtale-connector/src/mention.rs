//! Mention extraction — connectors that emit chat-like events
//! implement [`MentionExtractor`] so the runtime can harvest
//! destinations into the formation's shared mental model.
//!
//! ## Why a separate trait
//!
//! Every messaging connector emits events with different payload
//! shapes (Telegram `message.chat.id`, Discord `channel_id`,
//! Slack `channel`, etc.). The harvester can't know all of these
//! in advance, so each connector teaches it what to look for.
//!
//! The trait is intentionally minimal: no async, no I/O — pure
//! function from event payload to a vec of harvested destinations.
//! Connectors that don't emit chat-like events (cron, filesystem,
//! HTTP-only) skip implementing it; the runtime treats `None` as
//! "no destinations to harvest."
//!
//! ## Cooperation alignment
//!
//! Harvested destinations flow into the formation's
//! `external_workspaces` directory (see
//! `springtale-cooperation::mental_model::external_workspaces`).
//! This module is the connector-layer producer; the
//! cooperation-layer consumer translates the URI-shaped key into
//! a `WorkspaceKey` and applies the gossip-delta merge.

/// One destination found inside an event payload. The connector
/// produces these; the runtime harvester translates them into
/// `MentalModelWorkspaceRow` upserts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarvestedDestination {
    /// URI-shaped workspace key — must use the connector's scheme
    /// (see [`crate::workspace_key::scheme_for_connector`]).
    /// e.g. `"telegram://chat/12345"`, `"discord://guild/G/channel/C"`.
    pub workspace_key: String,
    /// Human-facing label for the dropdown — chat title, channel
    /// name, user handle. Sizes-only invariant: this is the ONLY
    /// human-readable text we persist per destination.
    pub display_name: String,
    /// `"user" | "group" | "channel" | "supergroup" | "dm" | "account" | "thread"`.
    pub kind: String,
    /// Connector-specific extras. Persisted as JSON in
    /// `mental_model_workspaces.metadata_json`. Privacy invariant:
    /// sizes-only — no message bodies, no member rosters past a count.
    pub metadata: serde_json::Value,
}

/// Extracts destinations from a connector's event payloads.
///
/// Pure function: no I/O, no async, no mutation. Receives a
/// `serde_json::Value` (the event's payload), inspects the
/// connector-specific fields, returns whatever destinations the
/// event "mentions". Empty vec for events that don't mention
/// anything (cron ticks, system events, etc.).
///
/// One method, intentionally small. Connectors implement this
/// once and the runtime calls it on every dispatched event.
pub trait MentionExtractor: Send + Sync + 'static {
    /// `trigger` is the trigger name (e.g. `"message_received"`,
    /// `"command_received"`). Lets the extractor cheaply early-
    /// return for triggers that never carry chat data. `payload`
    /// is the event's JSON body — typically the same shape the
    /// rule engine sees as `${trigger.*}`.
    fn extract(&self, trigger: &str, payload: &serde_json::Value) -> Vec<HarvestedDestination>;
}
