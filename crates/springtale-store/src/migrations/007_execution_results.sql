-- Migration 007: Execution results storage
--
-- Stores the actual output data from rule/action executions.
-- Events store metadata (what ran, when). Results store the data (what was returned).
-- Retention: capped at 100 per connector, oldest deleted on insert.

CREATE TABLE IF NOT EXISTS execution_results (
    id TEXT PRIMARY KEY,
    connector_name TEXT NOT NULL,
    rule_id TEXT,
    rule_name TEXT,
    output_json TEXT NOT NULL,
    success INTEGER NOT NULL DEFAULT 1,
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_execution_results_connector ON execution_results(connector_name, created_at DESC);
