-- D1 — External-workspace directory (extension of mental_model schema).
--
-- One entry per (formation, workspace_key). Stores what messaging
-- destinations the formation has discovered. Per COOPERATION.md §21
-- (Shared Mental Model) — the directory is formation-scoped and
-- gossip-replicated across formation members. A destination learned
-- by agent A in formation F is visible to agent B in formation F
-- automatically via the existing chitchat substrate.
--
-- Privacy invariant (matches the executions-log posture):
--   - display_name (chat title / channel name) is the only
--     human-readable text stored. No message bodies.
--   - metadata_json may carry a member count or username but
--     never a member roster.
--
-- Scoping:
--   - formation_id is REQUIRED — no global destinations. A user
--     with two formations sees two independent directories.
--   - DELETE CASCADE on formations(id) so dissolving a formation
--     drops its directory automatically.
--
-- Conflict resolution at write time happens in
-- `crates/springtale-cooperation/src/mental_model/external_workspaces.rs::merge_gossip_delta`.
-- This schema just stores the latest accepted row per key.

CREATE TABLE IF NOT EXISTS mental_model_workspaces (
    formation_id     TEXT    NOT NULL REFERENCES formations(id) ON DELETE CASCADE,
    workspace_key    TEXT    NOT NULL,             -- URI: "telegram://chat/12345"
    connector_name   TEXT    NOT NULL,
    display_name     TEXT    NOT NULL,
    kind             TEXT    NOT NULL,             -- "user" | "group" | "channel" | ...
    metadata_json    TEXT,                         -- serde_json::Value (nullable for empty {})
    first_seen_at    INTEGER NOT NULL,             -- unix ms
    last_seen_at     INTEGER NOT NULL,             -- unix ms; drives gossip merge
    provenance_json  TEXT    NOT NULL,             -- serialized WorkspaceProvenance
    PRIMARY KEY (formation_id, workspace_key)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_mm_workspaces_by_formation_and_connector
    ON mental_model_workspaces(formation_id, connector_name, last_seen_at DESC);

CREATE INDEX IF NOT EXISTS idx_mm_workspaces_by_connector
    ON mental_model_workspaces(connector_name, last_seen_at DESC);
