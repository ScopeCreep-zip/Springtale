-- Springtale schema v1
-- Phase 1a: SQLite tables for rules, connectors, events, and jobs.

CREATE TABLE IF NOT EXISTS _migrations (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS rules (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'enabled',
    version INTEGER NOT NULL DEFAULT 1,
    trigger_type TEXT NOT NULL,
    rule_toml TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_rules_trigger_type ON rules(trigger_type);
CREATE INDEX IF NOT EXISTS idx_rules_status ON rules(status);

CREATE TABLE IF NOT EXISTS connectors (
    name TEXT PRIMARY KEY,
    version TEXT NOT NULL,
    author TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    manifest_json TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    installed_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS events (
    id TEXT PRIMARY KEY,
    connector_name TEXT NOT NULL,
    trigger_type TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    action_taken TEXT NOT NULL DEFAULT ''
);

CREATE INDEX IF NOT EXISTS idx_events_connector ON events(connector_name);
CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp);

CREATE TABLE IF NOT EXISTS jobs (
    id TEXT PRIMARY KEY,
    payload TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    attempts INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 3,
    created_at TEXT NOT NULL,
    started_at TEXT,
    last_error TEXT
);

CREATE INDEX IF NOT EXISTS idx_jobs_status ON jobs(status);
