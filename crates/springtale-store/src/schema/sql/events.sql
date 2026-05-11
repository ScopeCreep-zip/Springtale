-- Event log — metadata about what fired and when.
-- Result payloads live in execution_results; this table is the
-- compact timeline used by the dashboard event stream.
CREATE TABLE IF NOT EXISTS events (
    id             TEXT PRIMARY KEY,
    connector_name TEXT NOT NULL,
    trigger_type   TEXT NOT NULL,
    timestamp      TEXT NOT NULL,
    action_taken   TEXT NOT NULL DEFAULT ''
);

CREATE INDEX IF NOT EXISTS idx_events_connector ON events(connector_name);
CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp);
