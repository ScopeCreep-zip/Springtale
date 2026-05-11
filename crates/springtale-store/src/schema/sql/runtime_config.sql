-- Runtime configuration store.
-- Holds settings that were previously frozen in springtale.toml.
-- Keys include 'ai_adapter', 'safety', 'heartbeat_interval_secs',
-- 'connector:{name}'. UI-driven changes don't require a restart.

CREATE TABLE IF NOT EXISTS config_store (
    key        TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT OR IGNORE INTO config_store (key, value_json) VALUES
    ('ai_adapter',              '{"type":"noop"}'),
    ('safety',                  '{}'),
    ('heartbeat_interval_secs', '1800');
