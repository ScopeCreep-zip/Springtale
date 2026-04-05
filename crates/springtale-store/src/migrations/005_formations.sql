-- Formations — swarms of cooperating agents.
-- Per COOPERATION.pdf: formations are peer groups that coordinate
-- through cadence, momentum, and awareness.
--
-- A formation is the user-facing abstraction. Internally it creates
-- rules that the engine evaluates. Users think in swarms, the system
-- executes rules.

CREATE TABLE IF NOT EXISTS formations (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    intent TEXT NOT NULL DEFAULT 'Reconnoiter',
    status TEXT NOT NULL DEFAULT 'draft',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS formation_members (
    id TEXT PRIMARY KEY,
    formation_id TEXT NOT NULL REFERENCES formations(id) ON DELETE CASCADE,
    connector_name TEXT NOT NULL,
    role_hint TEXT
);
