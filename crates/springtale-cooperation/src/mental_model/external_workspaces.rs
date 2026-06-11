//! External-workspace directory — the messaging chats, channels,
//! groups, and users a formation has discovered or been told about.
//!
//! Per `docs/intended-arch/COOPERATION.md §21`, the
//! `SharedMentalModel` is the formation-scoped, gossip-replicated
//! store of "what we collectively know about the world".
//! External destinations (Telegram chats, Discord channels, Signal
//! groups, IRC channels, Nostr pubkeys, Bluesky accounts) are
//! domain knowledge — the formation needs to know they exist
//! before it can dispatch messages to them.
//!
//! This module is the type layer for that knowledge. The
//! persistence layer (`mental_model_workspaces` table) lives in
//! `springtale-store`; the gossip-delta merge is defined here.
//!
//! ## Identification
//!
//! Each workspace is identified by a typed `WorkspaceKey` carrying
//! a URI string (see `springtale-connector::workspace_key`). The
//! URI format is `<connector_scheme>://<segment>/<id>` —
//! `telegram://chat/12345`, `discord://guild/G/channel/C`, etc.
//!
//! ## Provenance
//!
//! Every entry carries a [`WorkspaceProvenance`] audit tag
//! recording how it was discovered. The tag is preserved across
//! gossip replication so users can see "this destination came
//! from agent A who learned it via passive harvest of a `/start`
//! command" — even after the entry has propagated through three
//! formation members.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::cadence::AgentId;
use crate::types::WorkspaceKey;

/// One entry in the formation's external-workspace directory.
///
/// Sizes-only by privacy default — stores `display_name`
/// (chat title / channel name) and counters, never message
/// bodies or membership rosters past a count. Matches the
/// executions-log privacy posture.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExternalWorkspaceEntry {
    /// Connector that "owns" this workspace — only this connector
    /// can deposit/collect against the key.
    pub connector_name: String,
    /// Human-facing label surfaced in the picker dropdown.
    pub display_name: String,
    /// `"user" | "group" | "channel" | "supergroup" | "dm" | "account" | "thread"`.
    /// Drives the UI's icon + tooltip. Recipe field-kind filters
    /// reference this.
    pub kind: String,
    /// Connector-specific extras (Telegram username, Discord guild
    /// id, member count). Free-form JSON; the picker renders it as
    /// secondary metadata. Sizes-only invariant — no bodies, no
    /// member lists past a count.
    pub metadata: serde_json::Value,
    /// First time anyone in the formation observed this workspace.
    pub first_seen_at: DateTime<Utc>,
    /// Most-recent observation. Used by the gossip-delta merge as
    /// the conflict-resolution key.
    pub last_seen_at: DateTime<Utc>,
    /// Audit trail for the discovery channel.
    pub provenance: WorkspaceProvenance,
}

/// How the formation learned about this workspace. Surfaced in
/// the picker tooltip ("you /start'd this bot 3 minutes ago") and
/// the audit trail. `Gossiped` boxes the original provenance so
/// chains survive replication.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceProvenance {
    /// User typed the destination manually into a recipe input.
    ManualEntry { entered_by: AgentId },
    /// Harvested from an inbound event payload (user `/start`'d
    /// the bot, webhook arrived, message received, etc.).
    PassiveHarvest {
        /// What triggered the harvest — e.g. `"command_received:start"`,
        /// `"message_received"`. Drives the picker tooltip copy.
        trigger: String,
        at: DateTime<Utc>,
    },
    /// Returned by the connector's `discover_destinations` action.
    ActiveDiscovery { scanned_at: DateTime<Utc> },
    /// Replicated from another formation member's mental model
    /// via the gossip layer. Preserves the original discovery
    /// chain.
    Gossiped {
        from_agent: AgentId,
        original_provenance: Box<WorkspaceProvenance>,
    },
}

impl WorkspaceProvenance {
    /// Depth of the gossip chain. `0` for any non-gossiped
    /// provenance; `n` for a chain that's been replicated `n`
    /// times. The picker UI caps display depth at ~3 (cosmetic).
    pub fn gossip_depth(&self) -> usize {
        match self {
            Self::Gossiped {
                original_provenance,
                ..
            } => 1 + original_provenance.gossip_depth(),
            _ => 0,
        }
    }

    /// Walk the gossip chain to the original (non-gossiped)
    /// provenance. Used for "where did this entry really come
    /// from" audit queries.
    pub fn root(&self) -> &WorkspaceProvenance {
        match self {
            Self::Gossiped {
                original_provenance,
                ..
            } => original_provenance.root(),
            other => other,
        }
    }
}

/// A workspace discovered by a connector but not yet upserted into
/// the formation's mental model. The harvester / scan operation
/// converts these into [`ExternalWorkspaceEntry`]s before they
/// land in the store.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscoveredWorkspace {
    pub key: WorkspaceKey,
    pub display_name: String,
    pub kind: String,
    /// Connector-specific extras to bake into the entry's
    /// `metadata` field. Same privacy invariant as the entry —
    /// sizes-only.
    pub metadata: serde_json::Value,
}

/// Gossip-delta merge. Given an existing entry and an incoming
/// one for the same `WorkspaceKey`, pick the one to keep.
///
/// Rule: the entry with the most-recent `last_seen_at` wins.
/// When the incoming entry came over gossip and we have a more
/// recent local observation, we keep ours. When the incoming
/// entry is fresher, we wrap our provenance into a `Gossiped`
/// envelope so the audit chain survives.
///
/// Returns `None` when the existing entry should stay unchanged;
/// `Some(merged)` when the caller should write the merged entry
/// back. This shape lets the caller skip the SQL write on a
/// no-op merge.
pub fn merge_gossip_delta(
    existing: &ExternalWorkspaceEntry,
    incoming: &ExternalWorkspaceEntry,
    incoming_from_agent: AgentId,
) -> Option<ExternalWorkspaceEntry> {
    if incoming.last_seen_at <= existing.last_seen_at {
        // Local is at least as fresh — no change.
        return None;
    }
    // Incoming wins. Wrap its provenance in a Gossiped envelope so
    // the audit chain survives. If it's ALREADY a Gossiped from a
    // chain, just nest one deeper.
    let merged_provenance = WorkspaceProvenance::Gossiped {
        from_agent: incoming_from_agent,
        original_provenance: Box::new(incoming.provenance.clone()),
    };
    Some(ExternalWorkspaceEntry {
        connector_name: incoming.connector_name.clone(),
        display_name: incoming.display_name.clone(),
        kind: incoming.kind.clone(),
        metadata: incoming.metadata.clone(),
        // Preserve the original first_seen_at — earliest known.
        first_seen_at: existing.first_seen_at.min(incoming.first_seen_at),
        last_seen_at: incoming.last_seen_at,
        provenance: merged_provenance,
    })
}

/// The in-memory directory section of `SharedMentalModel`. A
/// `HashMap` keyed by `WorkspaceKey` so per-key lookups are O(1).
pub type ExternalWorkspaceDirectory = HashMap<WorkspaceKey, ExternalWorkspaceEntry>;

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    fn agent() -> AgentId {
        AgentId::new()
    }

    fn entry(last_seen: DateTime<Utc>, prov: WorkspaceProvenance) -> ExternalWorkspaceEntry {
        ExternalWorkspaceEntry {
            connector_name: "connector-telegram".into(),
            display_name: "Test Chat".into(),
            kind: "user".into(),
            metadata: json!({}),
            first_seen_at: last_seen,
            last_seen_at: last_seen,
            provenance: prov,
        }
    }

    #[test]
    fn provenance_round_trips_through_json() {
        let agent_a = agent();
        let prov = WorkspaceProvenance::ManualEntry {
            entered_by: agent_a,
        };
        let s = serde_json::to_string(&prov).unwrap();
        let back: WorkspaceProvenance = serde_json::from_str(&s).unwrap();
        assert_eq!(back, prov);
    }

    #[test]
    fn provenance_gossip_depth_counts_nesting() {
        let agent_a = agent();
        let agent_b = agent();
        let original = WorkspaceProvenance::ManualEntry {
            entered_by: agent_a,
        };
        let once = WorkspaceProvenance::Gossiped {
            from_agent: agent_b,
            original_provenance: Box::new(original.clone()),
        };
        let twice = WorkspaceProvenance::Gossiped {
            from_agent: agent(),
            original_provenance: Box::new(once.clone()),
        };
        assert_eq!(original.gossip_depth(), 0);
        assert_eq!(once.gossip_depth(), 1);
        assert_eq!(twice.gossip_depth(), 2);
    }

    #[test]
    fn provenance_root_unwraps_chain() {
        let agent_a = agent();
        let original = WorkspaceProvenance::ManualEntry {
            entered_by: agent_a,
        };
        let once = WorkspaceProvenance::Gossiped {
            from_agent: agent(),
            original_provenance: Box::new(original.clone()),
        };
        let twice = WorkspaceProvenance::Gossiped {
            from_agent: agent(),
            original_provenance: Box::new(once),
        };
        assert_eq!(twice.root(), &original);
    }

    #[test]
    fn merge_keeps_existing_when_incoming_is_older() {
        let now = Utc::now();
        let agent_a = agent();
        let existing = entry(
            now,
            WorkspaceProvenance::ManualEntry {
                entered_by: agent_a,
            },
        );
        let older = entry(
            now - chrono::Duration::seconds(10),
            WorkspaceProvenance::ManualEntry {
                entered_by: agent_a,
            },
        );
        assert!(merge_gossip_delta(&existing, &older, agent()).is_none());
    }

    #[test]
    fn merge_keeps_existing_on_tie() {
        let now = Utc::now();
        let agent_a = agent();
        let existing = entry(
            now,
            WorkspaceProvenance::ManualEntry {
                entered_by: agent_a,
            },
        );
        let same = entry(
            now,
            WorkspaceProvenance::ManualEntry {
                entered_by: agent_a,
            },
        );
        assert!(merge_gossip_delta(&existing, &same, agent()).is_none());
    }

    #[test]
    fn merge_picks_incoming_and_wraps_provenance() {
        let now = Utc::now();
        let agent_a = agent();
        let agent_b = agent();
        let existing = entry(
            now - chrono::Duration::seconds(10),
            WorkspaceProvenance::ManualEntry {
                entered_by: agent_a,
            },
        );
        let incoming = entry(
            now,
            WorkspaceProvenance::PassiveHarvest {
                trigger: "command_received:start".into(),
                at: now,
            },
        );
        let merged = merge_gossip_delta(&existing, &incoming, agent_b).unwrap();
        assert_eq!(merged.last_seen_at, incoming.last_seen_at);
        match merged.provenance {
            WorkspaceProvenance::Gossiped {
                from_agent,
                original_provenance,
            } => {
                assert_eq!(from_agent, agent_b);
                assert!(matches!(
                    *original_provenance,
                    WorkspaceProvenance::PassiveHarvest { .. }
                ));
            }
            _ => panic!("expected Gossiped"),
        }
    }

    #[test]
    fn merge_preserves_earliest_first_seen_across_gossip() {
        let earliest = Utc::now() - chrono::Duration::hours(24);
        let later = Utc::now() - chrono::Duration::hours(1);
        let now = Utc::now();
        let mut existing = entry(
            later,
            WorkspaceProvenance::ActiveDiscovery { scanned_at: later },
        );
        existing.first_seen_at = later;
        let mut incoming = entry(
            now,
            WorkspaceProvenance::ActiveDiscovery { scanned_at: now },
        );
        incoming.first_seen_at = earliest;
        let merged = merge_gossip_delta(&existing, &incoming, agent()).unwrap();
        assert_eq!(merged.first_seen_at, earliest);
    }

    #[test]
    fn discovered_workspace_round_trips() {
        let dw = DiscoveredWorkspace {
            key: WorkspaceKey::from("telegram://chat/12345"),
            display_name: "Alice's chat".into(),
            kind: "user".into(),
            metadata: json!({"username": "alice"}),
        };
        let s = serde_json::to_string(&dw).unwrap();
        let back: DiscoveredWorkspace = serde_json::from_str(&s).unwrap();
        assert_eq!(back, dw);
    }
}
