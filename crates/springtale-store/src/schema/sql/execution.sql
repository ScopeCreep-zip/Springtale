-- Execution results — the actual output payload from a rule/action.
-- events stores metadata (what ran, when); this table stores data.
-- Retention is enforced in code: capped at 100 per connector,
-- oldest deleted on insert.
CREATE TABLE IF NOT EXISTS execution_results (
    id             TEXT    PRIMARY KEY,
    connector_name TEXT    NOT NULL,
    rule_id        TEXT,
    rule_name      TEXT,
    output_json    TEXT    NOT NULL,
    success        INTEGER NOT NULL DEFAULT 1,
    error_message  TEXT,
    created_at     TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_execution_results_connector
    ON execution_results(connector_name, created_at DESC);
