-- Cooperation state persistence — momentum and rally per formation.
--
-- These tables replace the config-store hack (momentum:{id} keys)
-- with proper relational storage. Cascade-deletes ensure cleanup
-- when a formation is dissolved.

CREATE TABLE IF NOT EXISTS formation_momentum (
    formation_id TEXT PRIMARY KEY REFERENCES formations(id) ON DELETE CASCADE,
    tier TEXT NOT NULL DEFAULT 'Cold',
    consecutive_successes INTEGER NOT NULL DEFAULT 0,
    interference_count INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS formation_rally (
    formation_id TEXT PRIMARY KEY REFERENCES formations(id) ON DELETE CASCADE,
    tokens_remaining INTEGER NOT NULL DEFAULT 3,
    max_tokens INTEGER NOT NULL DEFAULT 3
);
