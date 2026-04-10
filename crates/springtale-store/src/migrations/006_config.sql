-- Migration 006: Runtime configuration store
--
-- Stores all configuration that was previously frozen in springtale.toml.
-- Enables UI-driven config changes without restart.
-- Keys: 'ai_adapter', 'safety', 'heartbeat_interval_secs', 'connector:{name}'

CREATE TABLE IF NOT EXISTS config_store (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Seed defaults
INSERT OR IGNORE INTO config_store (key, value_json) VALUES
    ('ai_adapter', '{"type":"noop"}'),
    ('safety', '{}'),
    ('heartbeat_interval_secs', '1800');
