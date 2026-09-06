-- Formations — swarms of cooperating agents.
-- Per docs/intended-arch/COOPERATION.pdf, a formation is a peer
-- group coordinated through cadence, momentum, and awareness.
-- Users think in swarms; the engine still evaluates rules underneath.
--
-- All formation-keyed tables live in this file so the foreign-key
-- chain is visible in one place. CASCADE deletes ensure that
-- dissolving a formation cleans up its members, momentum, and rally.

CREATE TABLE IF NOT EXISTS formations (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    intent     TEXT NOT NULL DEFAULT 'Reconnoiter',
    status     TEXT NOT NULL DEFAULT 'draft',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS formation_members (
    id             TEXT PRIMARY KEY,
    formation_id   TEXT NOT NULL REFERENCES formations(id) ON DELETE CASCADE,
    connector_name TEXT NOT NULL,
    role_hint      TEXT
);

-- A member is identified by its connector: `deploy_team` derives members from
-- the unique connector names, `delete_formation_member` deletes by connector,
-- and a member-owned rule (`RuleOwner::FormationMember`) is bound to its member
-- through that name. Enforce the invariant so a second row cannot make that
-- binding ambiguous.
CREATE UNIQUE INDEX IF NOT EXISTS idx_formation_members_connector
    ON formation_members(formation_id, connector_name);

CREATE TABLE IF NOT EXISTS formation_momentum (
    formation_id          TEXT    PRIMARY KEY REFERENCES formations(id) ON DELETE CASCADE,
    tier                  TEXT    NOT NULL DEFAULT 'Cold',
    consecutive_successes INTEGER NOT NULL DEFAULT 0,
    interference_count    INTEGER NOT NULL DEFAULT 0,
    updated_at            TEXT    NOT NULL
);

CREATE TABLE IF NOT EXISTS formation_rally (
    formation_id     TEXT    PRIMARY KEY REFERENCES formations(id) ON DELETE CASCADE,
    tokens_remaining INTEGER NOT NULL DEFAULT 3,
    max_tokens       INTEGER NOT NULL DEFAULT 3
);
